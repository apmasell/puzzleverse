use futures::{SinkExt, StreamExt};

#[derive(Default)]
struct Bookmarks(std::collections::HashMap<puzzleverse_core::BookmarkType, std::collections::BTreeSet<String>>);

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
struct Configuration {
  accounts: Vec<ServerConfiguration>,
  client: String,
  private_key: String,
  public_key: String,
}

enum ConnectionState {
  Idle,
  Active {
    inbound: futures::stream::Map<
      futures::stream::SplitStream<tokio_tungstenite::WebSocketStream<hyper::upgrade::Upgraded>>,
      fn(Result<tungstenite::Message, tungstenite::Error>) -> Option<puzzleverse_core::ClientResponse>,
    >,
    outbound: futures::stream::SplitSink<tokio_tungstenite::WebSocketStream<hyper::upgrade::Upgraded>, tungstenite::Message>,
  },
}

struct DirectMessageInfo {
  messages: Vec<puzzleverse_core::DirectMessage>,
  last_viewed: chrono::DateTime<chrono::Utc>,
  last_message: chrono::DateTime<chrono::Utc>,
}
#[derive(Default)]
struct DirectMessages(std::collections::BTreeMap<String, DirectMessageInfo>);

struct RealmInfo {
  last_updated: chrono::DateTime<chrono::Utc>,
  realms: Vec<puzzleverse_core::Realm>,
}
#[derive(Default)]
struct RealmsKnown(std::collections::HashMap<puzzleverse_core::RealmSource, RealmInfo>);

enum RealmSelector {
  Player,
  Local,
  Bookmarks,
  Remote(String),
  Url(String),
}

enum ScreenState {
  Error(String),
  Busy(String),
  InTransit,
  Loading {
    done: u32,
    total: u32,
  },
  Lost(RealmSelector, Option<String>),
  PasswordLogin {
    error_message: Option<String>,
    insecure: bool,
    password: String,
    player: String,
    server: String,
  },
  Realm {
    new_chat: Option<String>,
    confirm_delete: bool,
    direct_message: String,
    direct_message_user: String,
    is_mine: bool,
    messages: Vec<puzzleverse_core::RealmMessage>,
    realm_asset: String,
    realm_id: String,
    realm_message: String,
    realm_name: String,
    realm_selector: Option<RealmSelector>,
    realm_server: String,
  },
  ServerSelection {
    error_message: Option<String>,
    insecure: bool,
    player: String,
    server: String,
  },
  Waiting,
}

struct ServerConnection {
  inbound_rx: std::sync::Mutex<std::sync::mpsc::Receiver<ServerResponse>>,
  outbound_tx: std::sync::Mutex<tokio::sync::mpsc::UnboundedSender<ServerRequest>>,
  task: tokio::task::JoinHandle<()>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
struct ServerConfiguration {
  player: String,
  server: String,
}
#[derive(Clone)]
enum ServerRequest {
  CheckAuthMethods { insecure: bool, player: String, server: String },
  Deliver(puzzleverse_core::ClientRequest),
  PasswordLogin { insecure: bool, player: String, password: String, server: String },
}

enum ServerResponse {
  AuthMethod { insecure: bool, server: String, player: String, scheme: puzzleverse_core::AuthScheme },
  AuthMethodFailed { insecure: bool, server: String, player: String, error_message: String },
  AuthPasswordFailed { insecure: bool, server: String, player: String, password: String, error_message: String },
  Connected,
  Deliver(puzzleverse_core::ClientResponse),
  Disconnected,
}

impl ConnectionState {
  async fn process(&mut self, request: ServerRequest, response_stream: &mut std::sync::mpsc::Sender<ServerResponse>) {
    match request {
      ServerRequest::CheckAuthMethods { insecure, server, player } => {
        async fn make_request(server: &str, insecure: bool) -> Result<puzzleverse_core::AuthScheme, String> {
          match server.parse::<http::uri::Authority>() {
            Ok(authority) => {
              match hyper::Uri::builder()
                .scheme(if insecure { http::uri::Scheme::HTTP } else { http::uri::Scheme::HTTPS })
                .path_and_query("/api/auth/method")
                .authority(authority.clone())
                .build()
              {
                Ok(uri) => {
                  let connector = hyper_tls::HttpsConnector::new();
                  let client = hyper::Client::builder().build::<_, hyper::Body>(connector);

                  match client.request(hyper::Request::get(uri).body(hyper::Body::empty()).unwrap()).await {
                    Err(e) => Err(e.to_string()),
                    Ok(response) => {
                      use bytes::buf::Buf;
                      if response.status() == http::StatusCode::OK {
                        serde_json::from_slice(hyper::body::aggregate(response).await.map_err(|e| e.to_string())?.chunk()).map_err(|e| e.to_string())
                      } else {
                        let status = response.status();
                        match hyper::body::aggregate(response).await {
                          Err(e) => Err(format!("Failed to connect to {} {}: {}", server, status, e)),
                          Ok(buf) => Err(format!(
                            "Failed to connect to {} {}: {}",
                            server,
                            status,
                            std::str::from_utf8(buf.chunk()).unwrap_or("Bad UTF-8 data"),
                          )),
                        }
                      }
                    }
                  }
                }
                Err(e) => Err(e.to_string()),
              }
            }
            Err(e) => Err(e.to_string()),
          }
        }
        match make_request(&server, insecure).await {
          Ok(scheme) => response_stream.send(ServerResponse::AuthMethod { insecure, server, player, scheme }).unwrap(),
          Err(error_message) => response_stream.send(ServerResponse::AuthMethodFailed { insecure, server, player, error_message }).unwrap(),
        }
      }
      ServerRequest::PasswordLogin { insecure, server, player, password } => {
        async fn make_request(server: &str, insecure: bool, username: &str, password: &str) -> Result<ConnectionState, String> {
          match server.parse::<http::uri::Authority>() {
            Ok(authority) => {
              let token: String = match hyper::Uri::builder()
                .scheme(if insecure { http::uri::Scheme::HTTP } else { http::uri::Scheme::HTTPS })
                .path_and_query("/api/auth/password")
                .authority(authority.clone())
                .build()
              {
                Ok(uri) => {
                  let connector = hyper_tls::HttpsConnector::new();
                  let client = hyper::Client::builder().build::<_, hyper::Body>(connector);

                  match client
                    .request(
                      hyper::Request::post(uri)
                        .body(hyper::Body::from(
                          serde_json::to_vec(&puzzleverse_core::PasswordRequest { username, password }).map_err(|e| e.to_string())?,
                        ))
                        .unwrap(),
                    )
                    .await
                  {
                    Err(e) => Err(e.to_string()),
                    Ok(response) => {
                      use bytes::buf::Buf;
                      if response.status() == http::StatusCode::OK {
                        std::str::from_utf8(hyper::body::aggregate(response).await.map_err(|e| e.to_string())?.chunk())
                          .map_err(|e| e.to_string())
                          .map(|s| s.to_string())
                      } else {
                        let status = response.status();
                        match hyper::body::aggregate(response).await {
                          Err(e) => Err(format!("Failed to connect to {} {}: {}", server, status, e)),
                          Ok(buf) => Err(format!(
                            "Failed to connect to {} {}: {}",
                            server,
                            status,
                            std::str::from_utf8(buf.chunk()).unwrap_or("Bad UTF-8 data"),
                          )),
                        }
                      }
                    }
                  }
                }
                Err(e) => Err(e.to_string()),
              }?;
              eprintln!("Got authorization token: {}", &token);
              match hyper::Uri::builder()
                .scheme(if insecure { http::uri::Scheme::HTTP } else { http::uri::Scheme::HTTPS })
                .path_and_query("/api/client/v1")
                .authority(authority.clone())
                .build()
              {
                Ok(uri) => {
                  let connector = hyper_tls::HttpsConnector::new();
                  let client = hyper::Client::builder().build::<_, hyper::Body>(connector);
                  use rand::RngCore;

                  match client
                    .request(
                      hyper::Request::get(uri)
                        .version(http::Version::HTTP_11)
                        .header(http::header::CONNECTION, "upgrade")
                        .header(http::header::SEC_WEBSOCKET_VERSION, "13")
                        .header(http::header::SEC_WEBSOCKET_PROTOCOL, "puzzleverse")
                        .header(http::header::UPGRADE, "websocket")
                        .header(http::header::SEC_WEBSOCKET_KEY, format!("pv{}", (&mut rand::thread_rng()).next_u64()))
                        .header(http::header::AUTHORIZATION, format!("Bearer {}", token))
                        .body(hyper::Body::empty())
                        .unwrap(),
                    )
                    .await
                  {
                    Err(e) => Err(e.to_string()),
                    Ok(response) => {
                      if response.status() == http::StatusCode::SWITCHING_PROTOCOLS {
                        match hyper::upgrade::on(response).await {
                          Ok(upgraded) => {
                            fn decode_server_messages(
                              input: Result<tungstenite::Message, tungstenite::Error>,
                            ) -> Option<puzzleverse_core::ClientResponse> {
                              match input {
                                Ok(tungstenite::Message::Binary(value)) => match rmp_serde::from_read(std::io::Cursor::new(&value)) {
                                  Ok(v) => Some(v),
                                  Err(e) => {
                                    eprintln!("Failed to decode message from server. Mismatched protocols?: {}", e);
                                    None
                                  }
                                },
                                Ok(_) => None,
                                Err(e) => {
                                  eprintln!("Failed to decode Web Socket packet: {}", e);
                                  None
                                }
                              }
                            }
                            use futures::prelude::*;
                            let (writer, reader) = tokio_tungstenite::WebSocketStream::from_raw_socket(
                              upgraded,
                              tokio_tungstenite::tungstenite::protocol::Role::Client,
                              None,
                            )
                            .await
                            .split();
                            Ok(ConnectionState::Active { inbound: reader.map(decode_server_messages), outbound: writer })
                          }
                          Err(e) => Err(e.to_string()),
                        }
                      } else {
                        use bytes::buf::Buf;
                        let status = response.status();
                        match hyper::body::aggregate(response).await {
                          Err(e) => Err(format!("Failed to connect to {} {}: {}", server, status, e)),
                          Ok(buf) => Err(format!(
                            "Failed to connect to {} {}: {}",
                            server,
                            status,
                            std::str::from_utf8(buf.chunk()).unwrap_or("Bad UTF-8 data"),
                          )),
                        }
                      }
                    }
                  }
                }
                Err(e) => Err(e.to_string()),
              }
            }
            Err(e) => Err(e.to_string()),
          }
        }
        match make_request(&server, insecure, &player, &password).await {
          Ok(connection) => {
            *self = connection;
            response_stream.send(ServerResponse::Connected).unwrap();
          }
          Err(error_message) => {
            response_stream.send(ServerResponse::AuthPasswordFailed { insecure, server, player, password, error_message }).unwrap()
          }
        }
      }
      ServerRequest::Deliver(request) => {
        if let ConnectionState::Active { outbound, .. } = self {
          outbound.send(tungstenite::Message::Binary(rmp_serde::to_vec(&request).unwrap())).await.unwrap();
        }
      }
    }
  }
}

impl RealmSelector {
  fn draw_ui(
    &mut self,
    ui: &mut bevy_egui::egui::Ui,
    known_realms: bevy::ecs::system::ResMut<RealmsKnown>,

    server_requests: &mut bevy::app::EventWriter<ServerRequest>,
    request_for_realm: impl Fn(puzzleverse_core::RealmTarget) -> puzzleverse_core::ClientRequest,
  ) {
    let mut selected = self.id();
    ui.horizontal(|ui| {
      bevy_egui::egui::ComboBox::from_id_source("realm_selector").show_index(ui, &mut selected, 5, |i| match i {
        0 => "Personal".to_string(),
        1 => "Bookmarks".to_string(),
        2 => "Local".to_string(),
        3 => "Remote".to_string(),
        4 => "URL".to_string(),
        _ => panic!("Impossible realm selection."),
      });
    });
    if selected != self.id() {
      *self = match selected {
        0 => RealmSelector::Player,
        1 => RealmSelector::Bookmarks,
        2 => RealmSelector::Local,
        3 => RealmSelector::Remote(String::new()),
        4 => RealmSelector::Url(String::new()),
        _ => panic!("Impossible realm selection."),
      };
      if let Some(refresh_request) = self.refresh_request() {
        if known_realms.0.get(&refresh_request).map(|info| chrono::Utc::now() - info.last_updated > chrono::Duration::minutes(1)).unwrap_or(true) {
          server_requests.send(ServerRequest::Deliver(puzzleverse_core::ClientRequest::RealmsList(refresh_request)));
        }
      }
    }
    match self {
      RealmSelector::Player => {
        RealmSelector::show_list(ui, server_requests, request_for_realm, known_realms.0.get(&puzzleverse_core::RealmSource::Personal))
      }
      RealmSelector::Bookmarks => {
        RealmSelector::show_list(ui, server_requests, request_for_realm, known_realms.0.get(&puzzleverse_core::RealmSource::Bookmarks))
      }
      RealmSelector::Local => {
        RealmSelector::show_list(ui, server_requests, request_for_realm, known_realms.0.get(&puzzleverse_core::RealmSource::LocalServer))
      }
      RealmSelector::Remote(server) => {
        ui.horizontal(|ui| {
          let serverbox = ui.text_edit_singleline(server);
          let send = ui.button("⮨");
          if serverbox.changed() && server.ends_with('\n') || send.clicked() {
            use addr::parser::DomainName;
            if addr::psl::List.parse_domain_name(&server).is_ok() {
              server_requests.send(ServerRequest::Deliver(puzzleverse_core::ClientRequest::RealmsList(puzzleverse_core::RealmSource::RemoteServer(
                server.clone(),
              ))));
            }
          }
        });
        RealmSelector::show_list(
          ui,
          server_requests,
          request_for_realm,
          known_realms.0.get(&puzzleverse_core::RealmSource::RemoteServer(server.clone())),
        )
      }
      RealmSelector::Url(url) => {
        let (urlbox, send) = ui.horizontal(|ui| (ui.text_edit_singleline(url), ui.button("⮨"))).inner;

        match url.parse::<puzzleverse_core::RealmTarget>() {
          Ok(target) => {
            let source = puzzleverse_core::RealmSource::Manual(target);
            RealmSelector::show_list(ui, server_requests, request_for_realm, known_realms.0.get(&source));
            if urlbox.changed() && url.ends_with('\n') || send.clicked() {
              server_requests.send(ServerRequest::Deliver(puzzleverse_core::ClientRequest::RealmsList(source)));
            }
          }
          Err(e) => {
            ui.horizontal(|ui| {
              ui.add(
                bevy_egui::egui::Label::new(match e {
                  puzzleverse_core::RealmTargetParseError::BadPath => "Path is incorrect".to_string(),
                  puzzleverse_core::RealmTargetParseError::BadHost => "Host is incorrect".to_string(),
                  puzzleverse_core::RealmTargetParseError::BadSchema => "Only puzzleverse: URLs are supported".to_string(),
                  puzzleverse_core::RealmTargetParseError::UrlError(e) => e.to_string(),
                })
                .text_color(bevy_egui::egui::Color32::RED),
              )
            });
          }
        }
      }
    }
  }
  fn id(&self) -> usize {
    match self {
      RealmSelector::Player => 0,
      RealmSelector::Bookmarks => 1,
      RealmSelector::Local => 2,
      RealmSelector::Remote(_) => 3,
      RealmSelector::Url(_) => 4,
    }
  }
  fn refresh_request(&self) -> Option<puzzleverse_core::RealmSource> {
    match self {
      RealmSelector::Player => Some(puzzleverse_core::RealmSource::Personal),
      RealmSelector::Bookmarks => Some(puzzleverse_core::RealmSource::Bookmarks),
      RealmSelector::Local => Some(puzzleverse_core::RealmSource::LocalServer),
      RealmSelector::Remote(hostname) => {
        use addr::parser::DomainName;
        if addr::psl::List.parse_domain_name(&hostname).is_ok() {
          Some(puzzleverse_core::RealmSource::RemoteServer(hostname.clone()))
        } else {
          None
        }
      }
      RealmSelector::Url(url) => url.parse::<puzzleverse_core::RealmTarget>().ok().map(puzzleverse_core::RealmSource::Manual),
    }
  }
  fn show_list(
    ui: &mut bevy_egui::egui::Ui,
    server_requests: &mut bevy::app::EventWriter<ServerRequest>,
    request_for_realm: impl Fn(puzzleverse_core::RealmTarget) -> puzzleverse_core::ClientRequest,
    realms: Option<&RealmInfo>,
  ) {
    match realms {
      None => {
        ui.label("No matching realms.");
      }
      Some(realm_info) => {
        bevy_egui::egui::ScrollArea::auto_sized().id_source("realm_list").show(ui, |ui| {
          bevy_egui::egui::Grid::new("realm_list_grid").striped(true).spacing([10.0, 4.0]).show(ui, |ui| {
            for realm in &realm_info.realms {
              if ui
                .add(bevy_egui::egui::Label::new(&realm.name).strong())
                .on_hover_text(&realm.id)
                .on_hover_cursor(bevy_egui::egui::CursorIcon::PointingHand)
                .clicked()
              {
                server_requests.send(ServerRequest::Deliver(request_for_realm(match &realm.server {
                  None => puzzleverse_core::RealmTarget::LocalRealm(realm.id.clone()),
                  Some(server) => puzzleverse_core::RealmTarget::RemoteRealm { realm: realm.id.clone(), server: server.clone() },
                })));
              }
              ui.label(match &realm.server {
                Some(server) => bevy_egui::egui::Label::new(server.clone()),
                None => bevy_egui::egui::Label::new("(Local)").text_color(bevy_egui::egui::Color32::GRAY),
              });
              ui.label(realm.train.map(|t| format!("{}", t)).unwrap_or("".to_string()));
              ui.end_row();
            }
          });
        });
      }
    }
  }
}

impl ServerConnection {
  fn new(runtime: &tokio::runtime::Runtime) -> Self {
    let (outbound_tx, mut outbound_rx) = tokio::sync::mpsc::unbounded_channel();
    let (mut inbound_tx, inbound_rx) = std::sync::mpsc::channel();
    ServerConnection {
      outbound_tx: std::sync::Mutex::new(outbound_tx),
      inbound_rx: std::sync::Mutex::new(inbound_rx),
      task: runtime.spawn(async move {
        let mut state = ConnectionState::Idle;
        loop {
          enum Event {
            Server(Option<puzzleverse_core::ClientResponse>),
            UserInterface(Option<ServerRequest>),
          }
          let event = match &mut state {
            ConnectionState::Idle => Event::UserInterface(outbound_rx.recv().await),
            ConnectionState::Active { inbound, .. } => {
              tokio::select! {
                output = outbound_rx.recv() => Event::UserInterface(output),
                Some(response) = inbound.next() => Event::Server(response)
              }
            }
          };
          match event {
            Event::UserInterface(None) => break,
            Event::UserInterface(Some(output)) => state.process(output, &mut inbound_tx).await,
            Event::Server(output) => inbound_tx
              .send(match output {
                Some(message) => ServerResponse::Deliver(message),
                None => ServerResponse::Disconnected,
              })
              .unwrap(),
          }
        }
      }),
    }
  }
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
  let error_message = match self_update::backends::github::Update::configure()
    .repo_owner("apmasell")
    .repo_name("puzzleverse")
    .bin_name("puzzleverse-client")
    .show_download_progress(true)
    .current_version(self_update::cargo_crate_version!())
    .build()
    .unwrap()
    .update()
  {
    Ok(self_update::Status::UpToDate(_)) => None,
    Ok(self_update::Status::Updated(version)) => {
      println!("Updated to {}", version);
      None
    }
    Err(e) => Some(format!("Failed to update: {}", e)),
  };
  let dirs = directories::ProjectDirs::from("", "", "puzzleverse").unwrap();
  let mut login_file = std::path::PathBuf::new();
  login_file.extend(dirs.config_dir());
  login_file.push("client.json");
  let configuration = (if std::fs::metadata(&login_file).is_ok() {
    match std::fs::File::open(&login_file) {
      Ok(login_handle) => match serde_json::from_reader::<_, Configuration>(login_handle) {
        Ok(config) => Some(config),
        Err(e) => {
          eprintln!("Failed to load configuration: {}", e);
          None
        }
      },
      Err(e) => {
        eprintln!("Failed to open configuration: {}", e);
        None
      }
    }
  } else {
    None
  })
  .unwrap_or_else(|| {
    let keys = openssl::dsa::Dsa::generate(2048).expect("Unable to generate encryption key");
    let mut buf = [0; 32];
    openssl::rand::rand_bytes(&mut buf).unwrap();
    Configuration {
      accounts: vec![],
      client: buf.iter().map(|b| format!("{:2X}", b)).collect(),
      private_key: String::from_utf8(keys.private_key_to_pem().expect("Failed to encoding private key")).expect("OpenSSL generate invalid output"),
      public_key: String::from_utf8(keys.public_key_to_pem().expect("Failed to encoding public key")).expect("OpenSSL generate invalid output"),
    }
  });
  let mut insecure = false;
  {
    let mut ap = argparse::ArgumentParser::new();
    ap.set_description("Puzzleverse Client");
    ap.refer(&mut insecure).add_option(&["-i", "--insecure"], argparse::StoreTrue, "Use HTTP instead HTTPS");
    ap.parse_args_or_exit();
  }
  let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
  use bevy::ecs::system::IntoSystem;
  bevy::app::App::build()
    .add_event::<ServerRequest>()
    .add_event::<ServerResponse>()
    .insert_resource(ServerConnection::new(&rt))
    .insert_resource(ScreenState::ServerSelection { insecure, server: String::new(), player: String::new(), error_message })
    .init_resource::<Bookmarks>()
    .init_resource::<DirectMessages>()
    .init_resource::<i32>()
    .init_resource::<RealmsKnown>()
    .add_plugins(bevy::DefaultPlugins)
    .add_plugin(bevy_egui::EguiPlugin)
    .add_startup_system(setup.system())
    .add_system(draw_ui.system())
    .add_system(process_request.system())
    .add_system_to_stage(bevy::app::CoreStage::PreUpdate, send_network_events.system())
    .add_system_to_stage(bevy::app::CoreStage::PreUpdate, receive_network_events.system())
    .run();
  rt.shutdown_background();
}

#[cfg(target_arch = "wasm32")]
fn main() {
  unimplemented!()
}
fn setup(mut commands: bevy::ecs::system::Commands) {
  commands.spawn().insert_bundle(bevy::render::entity::PerspectiveCameraBundle::new_3d());
}

fn send_network_events(connection: bevy::ecs::system::ResMut<ServerConnection>, mut network_events: bevy::app::EventWriter<ServerResponse>) {
  network_events.send_batch(connection.inbound_rx.lock().unwrap().try_iter());
}

fn draw_ui(
  egui: bevy::ecs::system::ResMut<bevy_egui::EguiContext>,
  mut bookmarks: bevy::ecs::system::ResMut<Bookmarks>,
  mut clipboard: bevy::ecs::system::ResMut<bevy_egui::EguiClipboard>,
  mut direct_messages: bevy::ecs::system::ResMut<DirectMessages>,
  mut exit: bevy::app::EventWriter<bevy::app::AppExit>,
  known_realms: bevy::ecs::system::ResMut<RealmsKnown>,
  mut screen: bevy::ecs::system::ResMut<ScreenState>,
  mut server_count: bevy::ecs::system::ResMut<i32>,
  mut server_requests: bevy::app::EventWriter<ServerRequest>,
  mut windows: bevy::ecs::system::ResMut<bevy::window::Windows>,
) {
  let ui = egui.ctx();
  let mut next_ui = None;
  let window = windows.get_primary_mut().unwrap();
  let mut fullscreen = window.mode() == bevy::window::WindowMode::BorderlessFullscreen;

  window.set_title(match &mut *screen {
    ScreenState::InTransit => {
      bevy_egui::egui::CentralPanel::default().show(&ui, |ui| {
        ui.label("Finding Realm...");
      });
      "Finding realm - Puzzleverse".to_string()
    }
    ScreenState::Loading { done, total } => {
      bevy_egui::egui::CentralPanel::default().show(&ui, |ui| {
        ui.label("Loading Assets for Realm...");
        ui.label(format!("{}/{}", *done, *total));
      });
      "Loading realm - Puzzleverse".to_string()
    }
    ScreenState::Lost(realm_selector, error_message) => {
      bevy_egui::egui::Window::new("Navigate to Realm").collapsible(false).show(&ui, |ui| {
        if let Some(error_message) = error_message {
          ui.horizontal(|ui| ui.add(bevy_egui::egui::widgets::Label::new(error_message.clone()).text_color(bevy_egui::egui::Color32::RED)));
        }
        realm_selector.draw_ui(ui, known_realms, &mut server_requests, |realm| puzzleverse_core::ClientRequest::RealmChange { realm })
      });
      "Puzzleverse".into()
    }
    ScreenState::PasswordLogin { insecure, password, server, player, error_message } => {
      bevy_egui::egui::Window::new("Connect to Server").collapsible(false).show(&ui, |ui| {
        ui.horizontal(|ui| {
          ui.label("Password: ");
          ui.add(bevy_egui::egui::TextEdit::singleline(password).password(true));
        });
        if let Some(error_message) = error_message {
          ui.horizontal(|ui| ui.add(bevy_egui::egui::Label::new(error_message.as_str()).text_color(bevy_egui::egui::Color32::RED)));
        }
        ui.horizontal(|ui| {
          if ui.button("Connect").clicked() {
            server_requests.send(ServerRequest::PasswordLogin {
              insecure: *insecure,
              server: server.clone(),
              player: player.clone(),
              password: password.clone(),
            });
            next_ui = Some(ScreenState::Busy("Connecting...".into()));
          }
          if ui.button("Back").clicked() {
            next_ui = Some(ScreenState::ServerSelection { insecure: *insecure, server: server.clone(), player: player.clone(), error_message: None });
          }
          if ui.button("Quit").clicked() {
            exit.send(bevy::app::AppExit);
          }
        });
      });
      "Login - Puzzleverse".to_string()
    }
    ScreenState::Realm {
      new_chat,
      confirm_delete,
      direct_message,
      direct_message_user,
      is_mine,
      messages,
      realm_asset,
      realm_id,
      realm_message,
      realm_name,
      realm_selector,
      realm_server,
      ..
    } => {
      bevy_egui::egui::TopPanel::top("menu_bar").show(&ui, |ui| {
        if ui.button("🏠").clicked() {
          server_requests.send(ServerRequest::Deliver(puzzleverse_core::ClientRequest::RealmChange { realm: puzzleverse_core::RealmTarget::Home }));
        }
        if ui.button("⎈").clicked() {
          if realm_selector.is_none() {
            *realm_selector = Some(RealmSelector::Player);
          }
        }
        ui.checkbox(&mut fullscreen, "Fullscreen");
      });
      bevy_egui::egui::SidePanel::left("toolbar", 150.0).show(&ui, |ui| {
        bevy_egui::egui::CollapsingHeader::new("Realm").show(ui, |ui| {
          bevy_egui::egui::Grid::new("realm_grid").striped(true).spacing([40.0, 4.0]).show(ui, |ui| {
            ui.label("Name");
            ui.label(realm_name.as_str());
            ui.end_row();

            ui.label("URL");
            ui.label(realm_id.as_str());
            ui.end_row();

            if *is_mine {
              ui.add(bevy_egui::egui::Label::new("Danger!!!").text_color(bevy_egui::egui::Color32::RED));
              if ui.button("Delete").clicked() {
                *confirm_delete = true;
              }
              ui.end_row();
            } else {
              let realm_bookmarks = bookmarks.0.get_mut(&puzzleverse_core::BookmarkType::Realm);
              let url = puzzleverse_core::RealmTarget::RemoteRealm { realm: realm_id.clone(), server: realm_server.clone() }.to_url();
              let was_bookmarked = realm_bookmarks.map(|b| b.contains(realm_id)).unwrap_or(false);
              let mut is_bookmarked = was_bookmarked;
              ui.checkbox(&mut is_bookmarked, "Bookmarked");
              if was_bookmarked != is_bookmarked {
                server_requests.send(ServerRequest::Deliver((if is_bookmarked {
                  puzzleverse_core::ClientRequest::BookmarkAdd
                } else {
                  puzzleverse_core::ClientRequest::BookmarkRemove
                })(puzzleverse_core::BookmarkType::Realm, url)));
                server_requests.send(ServerRequest::Deliver(puzzleverse_core::ClientRequest::BookmarksGet(puzzleverse_core::BookmarkType::Realm)));
              }
              if ui.button("Go to My Instance").clicked() {
                server_requests.send(ServerRequest::Deliver(puzzleverse_core::ClientRequest::RealmChange {
                  realm: puzzleverse_core::RealmTarget::PersonalRealm(realm_asset.clone()),
                }));
              }
              ui.end_row();
            }
            if ui.button("Copy Link to Instance").clicked() {
              clipboard.set_contents(&puzzleverse_core::RealmTarget::RemoteRealm { realm: realm_id.clone(), server: realm_server.clone() }.to_url());
            }
            if ui.button("Copy Link to Personal Realm").clicked() {
              clipboard.set_contents(&puzzleverse_core::RealmTarget::PersonalRealm(realm_asset.clone()).to_url());
            }
          });
        });

        bevy_egui::egui::CollapsingHeader::new("Realm Chat").default_open(true).show(ui, |ui| {
          bevy_egui::egui::ScrollArea::auto_sized().id_source("realm_chat").show(ui, |ui| {
            bevy_egui::egui::Grid::new("realm_grid").striped(true).spacing([10.0, 4.0]).show(ui, |ui| {
              for message in messages {
                ui.label(&message.sender).on_hover_text(&message.timestamp.with_timezone(&chrono::Local).format("%c").to_string());
                ui.add(bevy_egui::egui::Label::new(message.body.clone()).wrap(true));
                ui.end_row();
              }
            })
          });
          ui.horizontal(|ui| {
            let chatbox = ui.text_edit_singleline(direct_message);
            let send = ui.button("⮨");
            if chatbox.changed() && direct_message.ends_with('\n') || send.clicked() {
              server_requests.send(ServerRequest::Deliver(puzzleverse_core::ClientRequest::DirectMessageSend {
                recipient: direct_message.clone(),
                id: todo!(),
                body: direct_message.clone(),
              }));
              direct_message.clear();
            }
          })
        });
        bevy_egui::egui::CollapsingHeader::new("Direct Chat").default_open(false).show(ui, |ui| {
          ui.horizontal(|ui| {
            bevy_egui::egui::ComboBox::from_id_source("direct_chat").selected_text(direct_message_user.as_str()).show_ui(ui, |ui| {
              for (user, info) in direct_messages.0.iter() {
                ui.selectable_value(
                  direct_message_user,
                  user.to_string(),
                  format!("{}{}", user, if info.last_viewed <= info.last_viewed { " *" } else { "" }),
                );
              }
            });
            if ui.button("⊞").clicked() && new_chat.is_none() {
              *new_chat = Some(String::new());
            }
          });
          bevy_egui::egui::ScrollArea::auto_sized().id_source("direct_chat").show(ui, |ui| {
            bevy_egui::egui::Grid::new("direct_grid").striped(true).spacing([10.0, 4.0]).show(ui, |ui| {
              match direct_messages.0.get_mut(direct_message_user).filter(|l| !l.messages.is_empty()) {
                None => {
                  ui.label("No messages");
                }
                Some(mut info) => {
                  info.last_message = chrono::Utc::now();
                  for message in info.messages.iter() {
                    ui.label(if message.inbound { direct_message_user.as_str() } else { "Me" })
                      .on_hover_text(&message.timestamp.with_timezone(&chrono::Local).format("%c").to_string());
                    ui.add(bevy_egui::egui::Label::new(message.body.to_string()).wrap(true));
                    ui.end_row();
                  }
                }
              }
            });
            let chatbox = ui.text_edit_singleline(realm_message);
            let send = ui.button("⮨");
            if chatbox.changed() && realm_message.ends_with('\n') || send.clicked() {
              server_requests.send(ServerRequest::Deliver(puzzleverse_core::ClientRequest::InRealm(puzzleverse_core::RealmRequest::SendMessage(
                realm_message.clone(),
              ))));
              realm_message.clear();
            }
          });
        });
      });
      if let Some(realm_selector) = realm_selector {
        bevy_egui::egui::Window::new("Travel to Realm").collapsible(false).show(&ui, |ui| {
          realm_selector.draw_ui(ui, known_realms, &mut server_requests, |realm| puzzleverse_core::ClientRequest::RealmChange { realm })
        });
      }
      let mut close_new_chat = false;
      if let Some(new_chat) = new_chat {
        bevy_egui::egui::Window::new("New Chat").collapsible(false).show(&ui, |ui| {
          ui.horizontal(|ui| ui.text_edit_singleline(new_chat));
          ui.horizontal(|ui| {
            if ui.button("Start").clicked() && !new_chat.is_empty() {
              match direct_messages.0.entry(new_chat.clone()) {
                std::collections::btree_map::Entry::Occupied(_) => (),
                std::collections::btree_map::Entry::Vacant(v) => {
                  v.insert(DirectMessageInfo { messages: vec![], last_viewed: chrono::MIN_DATETIME, last_message: chrono::MIN_DATETIME });
                }
              }
              close_new_chat = true;
            }
            if ui.button("Cancel").clicked() {
              close_new_chat = true;
            }
          });
        });
      }
      if close_new_chat {
        *new_chat = None;
      }
      if *confirm_delete {
        bevy_egui::egui::Window::new("Delete Realm").collapsible(false).show(&ui, |ui| {
          ui.horizontal(|ui| ui.label("Are you sure you want to delete this realm?"));
          ui.horizontal(|ui| {
            if ui.button("Delete").clicked() {
              *confirm_delete = false;
              *server_count += 1;
              let id = *server_count;
              server_requests.send(ServerRequest::Deliver(puzzleverse_core::ClientRequest::RealmDelete { id, target: realm_id.clone() }));
            }
            if ui.button("Cancel").clicked() {
              *confirm_delete = false;
            }
          });
        });
      }
      format!("{} - Puzzleverse", realm_name)
    }
    ScreenState::ServerSelection { insecure, server, player, error_message } => {
      bevy_egui::egui::Window::new("Connect to Server").collapsible(false).show(&ui, |ui| {
        bevy_egui::egui::Grid::new("connect_grid").striped(true).spacing([10.0, 8.0]).show(ui, |ui| {
          ui.label("Server: ");
          ui.add(bevy_egui::egui::TextEdit::singleline(server).desired_width(300.0));
          ui.end_row();
          ui.label("Player: ");
          ui.add(bevy_egui::egui::TextEdit::singleline(player).desired_width(300.0));
          ui.end_row();
          if let Some(error_message) = error_message {
            ui.label("Error: ");
            ui.add(bevy_egui::egui::Label::new(error_message.as_str()).text_color(bevy_egui::egui::Color32::RED));
            ui.end_row();
          }
          if *insecure {
            ui.label("Warning: ");
            ui.add(bevy_egui::egui::Label::new("Connection is unencrypted. I hope this is for debugging.").text_color(bevy_egui::egui::Color32::RED));
            ui.end_row();
          }
          if ui.button("Connect").clicked() {
            server_requests.send(ServerRequest::CheckAuthMethods { insecure: *insecure, server: server.clone(), player: player.clone() });
            next_ui = Some(ScreenState::Busy(format!("Contacting {}...", &server)))
          }
          if ui.button("Quit").clicked() {
            exit.send(bevy::app::AppExit);
          }
          ui.end_row();
          ui.label(bevy_egui::egui::Label::new(format!("v{}", self_update::cargo_crate_version!())).text_style(bevy_egui::egui::TextStyle::Small));
        });
      });
      "Login - Puzzleverse".to_string()
    }
    ScreenState::Busy(message) => {
      bevy_egui::egui::CentralPanel::default().show(&ui, |ui| {
        ui.label(message.as_str());
      });
      "Puzzleverse".to_string()
    }
    ScreenState::Waiting => {
      bevy_egui::egui::CentralPanel::default().show(&ui, |ui| {
        ui.label("Connecting...");
      });
      "Connecting - Puzzleverse".to_string()
    }
    ScreenState::Error(error) => {
      bevy_egui::egui::CentralPanel::default().show(&ui, |ui| {
        ui.label(error.as_str());
        if ui.button("Reconnect").clicked() {
          next_ui = Some(ScreenState::ServerSelection { error_message: None, insecure: false, player: String::new(), server: String::new() })
        }
        if ui.button("Quit").clicked() {
          exit.send(bevy::app::AppExit);
        }
      });
      "Error - Puzzleverse".to_string()
    }
  });
  window.set_mode(if fullscreen { bevy::window::WindowMode::BorderlessFullscreen } else { bevy::window::WindowMode::Windowed });
  if let Some(value) = next_ui {
    *screen = value;
  }
}
fn process_request(
  mut bookmarks: bevy::ecs::system::ResMut<Bookmarks>,
  mut direct_messages: bevy::ecs::system::ResMut<DirectMessages>,
  mut exit: bevy::app::EventWriter<bevy::app::AppExit>,
  mut known_realms: bevy::ecs::system::ResMut<RealmsKnown>,
  mut screen: bevy::ecs::system::ResMut<ScreenState>,
  mut server_requests: bevy::app::EventWriter<ServerRequest>,
  mut server_responses: bevy::app::EventReader<ServerResponse>,
) {
  for response in server_responses.iter() {
    match response {
      ServerResponse::AuthMethod { insecure, server, player, scheme: puzzleverse_core::AuthScheme::Password } => {
        *screen = ScreenState::PasswordLogin {
          insecure: *insecure,
          server: server.clone(),
          player: player.clone(),
          password: String::new(),
          error_message: None,
        };
      }
      ServerResponse::AuthMethodFailed { insecure, server, player, error_message } => {
        *screen = ScreenState::ServerSelection {
          insecure: *insecure,
          server: server.clone(),
          player: player.clone(),
          error_message: Some(error_message.clone()),
        };
      }
      ServerResponse::AuthPasswordFailed { insecure, server, player, password, error_message } => {
        *screen = ScreenState::PasswordLogin {
          insecure: *insecure,
          server: server.clone(),
          player: player.clone(),
          password: password.clone(),
          error_message: Some(error_message.clone()),
        };
      }
      ServerResponse::Connected => {
        *screen = ScreenState::Waiting;
        server_requests.send_batch(
          vec![
            ServerRequest::Deliver(puzzleverse_core::ClientRequest::Capabilities),
            ServerRequest::Deliver(puzzleverse_core::ClientRequest::DirectMessageStats),
            ServerRequest::Deliver(puzzleverse_core::ClientRequest::BookmarksGet(puzzleverse_core::BookmarkType::ConsensualEmote)),
            ServerRequest::Deliver(puzzleverse_core::ClientRequest::BookmarksGet(puzzleverse_core::BookmarkType::DirectedEmote)),
            ServerRequest::Deliver(puzzleverse_core::ClientRequest::BookmarksGet(puzzleverse_core::BookmarkType::Emote)),
            ServerRequest::Deliver(puzzleverse_core::ClientRequest::BookmarksGet(puzzleverse_core::BookmarkType::Realm)),
            ServerRequest::Deliver(puzzleverse_core::ClientRequest::BookmarksGet(puzzleverse_core::BookmarkType::RealmAsset)),
            ServerRequest::Deliver(puzzleverse_core::ClientRequest::BookmarksGet(puzzleverse_core::BookmarkType::Server)),
          ]
          .into_iter(),
        )
      }
      ServerResponse::Disconnected => {
        *screen = ScreenState::ServerSelection { insecure: false, server: String::new(), player: String::new(), error_message: None };
      }
      ServerResponse::Deliver(puzzleverse_core::ClientResponse::Bookmarks(key, values)) => {
        bookmarks.0.insert(*key, values.iter().cloned().collect());
        if key == &puzzleverse_core::BookmarkType::Player {
          for player in values.iter() {
            match direct_messages.0.entry(player.clone()) {
              std::collections::btree_map::Entry::Occupied(_) => (),
              std::collections::btree_map::Entry::Vacant(v) => {
                v.insert(DirectMessageInfo { last_viewed: chrono::MIN_DATETIME, messages: vec![], last_message: chrono::MIN_DATETIME });
              }
            }
          }
        }
      }

      ServerResponse::Deliver(puzzleverse_core::ClientResponse::DirectMessages { player, messages }) => match direct_messages.0.entry(player.clone())
      {
        std::collections::btree_map::Entry::Occupied(mut o) => {
          let existing_messages = &mut o.get_mut().messages;
          existing_messages.extend(messages.clone());
          existing_messages.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
          existing_messages.dedup_by(|a, b| a.inbound == b.inbound && a.body.eq(&b.body));
        }
        std::collections::btree_map::Entry::Vacant(v) => {
          v.insert(DirectMessageInfo {
            last_viewed: chrono::MIN_DATETIME,
            messages: messages.clone(),
            last_message: messages.iter().map(|m| m.timestamp).max().unwrap_or(chrono::MIN_DATETIME),
          });
        }
      },
      ServerResponse::Deliver(puzzleverse_core::ClientResponse::DirectMessageReceived { sender, body, timestamp }) => {
        let message = puzzleverse_core::DirectMessage { inbound: true, body: body.clone(), timestamp: *timestamp };
        match direct_messages.0.entry(sender.clone()) {
          std::collections::btree_map::Entry::Occupied(mut o) => {
            o.get_mut().messages.push(message);
            o.get_mut().last_message = *timestamp
          }
          std::collections::btree_map::Entry::Vacant(v) => {
            v.insert(DirectMessageInfo { last_viewed: chrono::MIN_DATETIME, messages: vec![message], last_message: *timestamp });
          }
        }
      }
      ServerResponse::Deliver(puzzleverse_core::ClientResponse::DirectMessageStats { stats, last_login }) => {
        for (sender, timestamp) in stats {
          match direct_messages.0.entry(sender.clone()) {
            std::collections::btree_map::Entry::Occupied(mut o) => {
              o.get_mut().last_message = *timestamp;
            }
            std::collections::btree_map::Entry::Vacant(v) => {
              v.insert(DirectMessageInfo { last_viewed: *last_login, messages: vec![], last_message: *timestamp });
            }
          }
        }
      }
      ServerResponse::Deliver(puzzleverse_core::ClientResponse::Disconnect) => {
        exit.send(bevy::app::AppExit);
      }
      ServerResponse::Deliver(puzzleverse_core::ClientResponse::RealmsAvailable { display, realms }) => {
        known_realms.0.insert(display.clone(), RealmInfo { last_updated: chrono::Utc::now(), realms: realms.clone() });
      }
      ServerResponse::Deliver(puzzleverse_core::ClientResponse::InTransit) => {
        *screen = ScreenState::InTransit;
      }
      ServerResponse::Deliver(puzzleverse_core::ClientResponse::RealmChanged(change)) => match change {
        puzzleverse_core::RealmChange::Denied => {
          *screen = ScreenState::Lost(RealmSelector::Local, Some("Cannot travel to the realm you requested.".to_string()));
        }
        puzzleverse_core::RealmChange::Success { capabilities, .. } => {
          let missing_capabilities: Vec<_> =
            capabilities.iter().filter(|c| !puzzleverse_core::CAPABILITIES.contains(&c.as_str())).map(|c| c.as_str()).collect();
          if missing_capabilities.is_empty() {
            todo!();
          } else {
            *screen = ScreenState::Lost(
              RealmSelector::Local,
              Some(format!("Your client does not support {} required by this realm.", missing_capabilities.join(" nor "))),
            );
          }
        }
      },
      ServerResponse::Deliver(x) => eprintln!("Got unhandled request: {:?}", x),
    }
  }
}

fn receive_network_events(connection: bevy::ecs::system::ResMut<ServerConnection>, mut network_events: bevy::app::EventReader<ServerRequest>) {
  for message in network_events.iter() {
    if let Err(e) = (*connection).outbound_tx.lock().unwrap().send(message.clone()) {
      panic!("Failed to send to server monitoring process: {}", e);
    }
  }
}
