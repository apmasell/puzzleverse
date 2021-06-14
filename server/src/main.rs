mod auth;
mod html;
mod player_state;
mod puzzle;
mod realm;
mod schema;
mod views;

#[macro_use]
extern crate diesel;
use self::diesel::prelude::*;
use diesel::connection::Connection;
#[macro_use]
extern crate diesel_migrations;
use futures::prelude::*;
use rand::{Rng, RngCore};
use serde::{Deserialize, Serialize};
use std::io::prelude::*;

embed_migrations!();

slotmap::new_key_type! { pub struct PlayerKey; }
slotmap::new_key_type! { pub struct RealmKey; }
slotmap::new_key_type! { pub struct RemoteKey; }

sql_function! { #[sql_name = ""]fn sql_not_null_bytes(x: diesel::sql_types::Nullable<diesel::sql_types::Binary>) -> diesel::sql_types::Binary}
sql_function! { #[sql_name = ""]fn sql_not_null_int(x: diesel::sql_types::Nullable<diesel::sql_types::Integer>) -> diesel::sql_types::Integer}
sql_function! { #[sql_name = ""]fn sql_not_null_str(x: diesel::sql_types::Nullable<diesel::sql_types::VarChar>) -> diesel::sql_types::VarChar}
lazy_static::lazy_static! {
    static ref BUILD_ID: prometheus::IntCounterVec =
        prometheus::register_int_counter_vec!("puzzleverse_build_id", "Current server build ID.", &["build_id"]).unwrap();
}
lazy_static::lazy_static! {
    static ref BAD_CLIENT_REQUESTS: prometheus::IntCounterVec =
        prometheus::register_int_counter_vec!("puzzleverse_bad_client_requests", "Number of client requests that couldn't be decoded.", &["player"]).unwrap();
}

lazy_static::lazy_static! {
    static ref BAD_JWT: prometheus::IntCounter=
        prometheus::register_int_counter!("puzzleverse_bad_jwt", "Number of times a bad JWT was received from a client or server.").unwrap();
}
lazy_static::lazy_static! {
    static ref BAD_LINK: prometheus::IntCounter=
        prometheus::register_int_counter!("puzzleverse_bad_link", "Number of times a player was moved to an invalid link.").unwrap();
}
lazy_static::lazy_static! {
    static ref BAD_SERVER_REQUESTS: prometheus::IntCounterVec =
        prometheus::register_int_counter_vec!("puzzleverse_bad_server_requests", "Number of server requests that couldn't be decoded.", &["player"]).unwrap();
}
lazy_static::lazy_static! {
    static ref BAD_WEB_REQUEST: prometheus::IntCounter=
        prometheus::register_int_counter!("puzzleverse_bad_web_request", "Number of invalid HTTP requests.").unwrap();
}
lazy_static::lazy_static! {
    static ref FAILED_CLIENT_EVICT: prometheus::IntCounterVec =
        prometheus::register_int_counter_vec!("puzzleverse_failed_client_evict", "Number of client connections that produced an error while being evicted.", &["player"]).unwrap();
}
lazy_static::lazy_static! {
    static ref FAILED_SERVER_CALLBACK: prometheus::IntCounterVec =
        prometheus::register_int_counter_vec!("puzzleverse_failed_server_callback", "Number of times a server asked for a connection and then failed to be accessible.", &["server"]).unwrap();
}
lazy_static::lazy_static! {
    static ref FAILED_SERVER_EVICT: prometheus::IntCounterVec =
        prometheus::register_int_counter_vec!("puzzleverse_failed_server_evict", "Number of server connections that produced an error while being evicted.", &["server"]).unwrap();
}
lazy_static::lazy_static! {
static ref DEFAULT_DENY_ACCESS: Vec<u8> = {
    let value: AccessControlSetting = ( puzzleverse_core::AccessDefault::Deny, vec![],);
    rmp_serde::encode::to_vec(&value).unwrap()
};
}
lazy_static::lazy_static! {
static ref DEFAULT_LOCAL_ONLY_ACCESS: Vec<u8> = {
    let value: AccessControlSetting = (
        puzzleverse_core::AccessDefault::Deny,
        vec![puzzleverse_core::AccessControl::AllowLocal(None)],
    );
    rmp_serde::encode::to_vec(&value).unwrap()
};
}
const CAPABILITIES: &[&str] = &["base"];

const DEFAULT_HOME: &str = "DEFAULT_HOME";
pub type AccessControlSetting = (puzzleverse_core::AccessDefault, Vec<puzzleverse_core::AccessControl>);

pub enum AssetPullAction {
  PushToPlayer(PlayerKey),
  LoadRealm(String, std::sync::Arc<std::sync::atomic::AtomicUsize>),
}

type OutgoingConnection<T> = futures::sink::With<
  futures::stream::SplitSink<tokio_tungstenite::WebSocketStream<hyper::upgrade::Upgraded>, tokio_tungstenite::tungstenite::protocol::Message>,
  tokio_tungstenite::tungstenite::protocol::Message,
  T,
  futures::future::Ready<Result<tokio_tungstenite::tungstenite::protocol::Message, tungstenite::Error>>,
  fn(T) -> futures::future::Ready<Result<tokio_tungstenite::tungstenite::protocol::Message, tungstenite::Error>>,
>;
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
struct PlayerClaim {
  exp: usize,
  name: String,
}

enum RealmKind {
  Loaded(RealmKey),
  WaitingAsset { asset: String, waiters: Vec<(PlayerKey, u64)> },
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
enum ReleaseTarget {
  Transit,
  Home,
  Realm(String, String),
}

enum RealmMove {
  ToHome(PlayerKey),
  ToExistingRealm { player: PlayerKey, realm: String, server: Option<String> },
  ToRealm { player: PlayerKey, owner: String, asset: String },
}
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
struct RemoteClaim {
  exp: usize,
  name: String,
}
enum RemoteConnection {
  Online(OutgoingConnection<RemoteMessage>),
  Dead(chrono::DateTime<chrono::Utc>, Vec<RemoteMessage>),
  Offline,
}

struct RemoteState {
  connection: tokio::sync::Mutex<RemoteConnection>,
  interested_in_list: tokio::sync::Mutex<std::collections::HashSet<PlayerKey>>,
  name: String,
}
/// Messages exchanged between servers; all though there is a client/server relationship implied by Web Sockets, the connection is peer-to-peer, therefore, there is no distinction between requests and responses in this structure
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
enum RemoteMessage {
  /// Request assets from this server if they are available
  AssetsPull { assets: Vec<String> },
  /// Send assets requested by the other server
  AssetsPush { assets: std::collections::HashMap<String, puzzleverse_core::asset::Asset> },
  /// Transfer direct messages
  DirectMessage(Vec<RemoteDirectMessage>),
  /// Check the online status of a player
  OnlineStatusRequest { requester: String, target: String },
  /// Send the online status of a player
  OnlineStatusResponse { requester: String, target: String, state: puzzleverse_core::PlayerLocationState },
  /// Indicate that a realm change has occurred; if the realm change was not successful, the remote server has relinquished control of the player
  RealmChanged { player: String, change: puzzleverse_core::RealmChange },
  /// Process a realm-related request for a player that has been handed off to this server
  RealmRequest { player: String, request: puzzleverse_core::RealmRequest },
  /// Receive a realm-related response for a player that has been handed off to this server
  RealmResponse { player: String, response: puzzleverse_core::RealmResponse },
  /// List realms that are in the public directory for this server
  RealmsList,
  /// The realms that are available in the public directory on this server
  RealmsAvailable(Vec<puzzleverse_core::Realm>),
  /// For a visitor, indicate what assets the client will require
  VisitorCheckAssets { player: String, assets: Vec<String> },
  /// Releases control of a player to the originating server
  VisitorRelease(String, ReleaseTarget),
  /// Send player to a realm on the destination server
  ///
  /// This transfers control of that player to the remote server until the originating server yanks them back or the destination server send them back
  VisitorSend { player: String, realm: String },
  /// Forces a player to be removed from a remote server by the originating server
  VisitorYank(String),
}
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
struct RemoteDirectMessage {
  sender: String,
  recipient: String,
  timestamp: chrono::DateTime<chrono::Utc>,
  body: String,
}
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
struct RemoteConnectRequest {
  token: String,
  server: String,
}

/// The state management for the server
pub struct Server {
  access_acl: std::sync::Arc<tokio::sync::Mutex<crate::AccessControlSetting>>,
  admin_acl: std::sync::Arc<tokio::sync::Mutex<crate::AccessControlSetting>>,
  asset_store: Box<dyn puzzleverse_core::asset_store::AssetStore>,
  attempting_remote_contacts: tokio::sync::Mutex<std::collections::HashSet<String>>,
  /// The key to check JWT from users during authorization
  /// The authentication provider that can determine what users can get a JWT to log in
  authentication: std::sync::Arc<dyn auth::AuthProvider>,
  /// A pool of database connections to be used as required
  db_pool: r2d2::Pool<diesel::r2d2::ConnectionManager<diesel::pg::PgConnection>>,
  jwt_decoding_key: jsonwebtoken::DecodingKey<'static>,
  /// The key to create JWT for users during authentication
  jwt_encoding_key: jsonwebtoken::EncodingKey,
  message_acl: std::sync::Arc<tokio::sync::Mutex<crate::AccessControlSetting>>,
  move_epoch: std::sync::atomic::AtomicU64,
  move_queue: tokio::sync::Mutex<tokio::sync::mpsc::Sender<RealmMove>>,
  /// The self-hostname for this server
  name: String,
  outstanding_assets: tokio::sync::Mutex<std::collections::HashMap<String, Vec<AssetPullAction>>>,
  /// While these need to match against a database record, they do not have the same information. Some information about a player is transient and lost if the server is restarted.
  players: tokio::sync::RwLock<std::collections::HashMap<String, PlayerKey>>,
  /// The state information of active players
  player_states: tokio::sync::RwLock<slotmap::DenseSlotMap<PlayerKey, player_state::PlayerState>>,
  push_assets: tokio::sync::Mutex<tokio::sync::mpsc::Sender<(PlayerKey, String)>>,
  /// The state of information about active realms
  realms: tokio::sync::Mutex<std::collections::HashMap<String, RealmKind>>,
  realm_states: tokio::sync::RwLock<slotmap::DenseSlotMap<RealmKey, realm::RealmState>>,
  remote_states: tokio::sync::RwLock<slotmap::DenseSlotMap<RemoteKey, RemoteState>>,
  remotes: tokio::sync::RwLock<std::collections::HashMap<String, RemoteKey>>,
  startup: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize, Deserialize)]
pub struct ServerConfiguration {
  asset_store: String,
  authentication: crate::auth::AuthConfiguration,
  bind_address: Option<String>,
  certificate: Option<std::path::PathBuf>,
  database_url: String,
  name: String,
}
fn abs_difference<T: std::ops::Sub<Output = T> + Ord>(x: T, y: T) -> T {
  if x < y {
    y - x
  } else {
    x - y
  }
}

fn connection_has(value: &http::header::HeaderValue, needle: &str) -> bool {
  if let Ok(v) = value.to_str() {
    v.split(',').any(|s| s.trim().eq_ignore_ascii_case(needle))
  } else {
    false
  }
}

fn convert_key(input: &[u8]) -> String {
  const WS_GUID: &[u8] = b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
  let mut digest = sha1::Sha1::new();
  digest.update(input);
  digest.update(WS_GUID);
  base64::encode(&digest.digest().bytes())
}

pub fn encode_message<T: Serialize + Sized>(
  input: T,
) -> futures::future::Ready<Result<tokio_tungstenite::tungstenite::protocol::Message, tungstenite::Error>> {
  futures::future::ready(match rmp_serde::to_vec(&input) {
    Ok(data) => Ok(tungstenite::Message::Binary(data)),
    Err(e) => Err(tungstenite::Error::Protocol(std::borrow::Cow::Owned(format!("{}", e)))),
  })
}

fn etag_request(
  content_type: &'static str,
  contents: &'static [u8],
  req: http::Request<hyper::Body>,
) -> Result<http::Response<hyper::Body>, http::Error> {
  if req.headers().get("If-None-Match").map(|tag| tag.to_str().ok()).flatten().map(|v| v == build_id::get().to_string()).unwrap_or(false) {
    http::Response::builder().status(304).body(hyper::Body::empty())
  } else {
    http::Response::builder()
      .status(http::StatusCode::OK)
      .header("Content-Type", content_type)
      .header("ETag", build_id::get().to_string())
      .body(hyper::body::Bytes::from_static(contents).into())
  }
}
fn id_for_type(bookmark_type: &puzzleverse_core::BookmarkType) -> &'static str {
  match bookmark_type {
    puzzleverse_core::BookmarkType::Asset => "a",
    puzzleverse_core::BookmarkType::ConsensualEmote => "c",
    puzzleverse_core::BookmarkType::DirectedEmote => "E",
    puzzleverse_core::BookmarkType::Emote => "e",
    puzzleverse_core::BookmarkType::Player => "p",
    puzzleverse_core::BookmarkType::Realm => "r",
    puzzleverse_core::BookmarkType::RealmAsset => "R",
    puzzleverse_core::BookmarkType::Server => "s",
  }
}
fn jwt_expiry_time() -> usize {
  (std::time::SystemTime::now() + std::time::Duration::from_secs(3600)).duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as usize
}
fn load_cert<P: AsRef<std::path::Path>>(certificate_path: P) -> Result<tokio_native_tls::TlsAcceptor, String> {
  let mut f = std::fs::File::open(&certificate_path).map_err(|e| e.to_string())?;
  let mut buffer = Vec::new();
  f.read_to_end(&mut buffer).map_err(|e| e.to_string())?;
  let cert = native_tls::Identity::from_pkcs12(&buffer, "").map_err(|e| e.to_string())?;
  Ok(tokio_native_tls::TlsAcceptor::from(native_tls::TlsAcceptor::builder(cert).build().map_err(|e| e.to_string())?))
}

impl RemoteConnection {
  async fn send(&mut self, message: RemoteMessage) -> () {
    match self {
      RemoteConnection::Online(_) => {}
      RemoteConnection::Dead(time, queued) => {
        if chrono::Utc::now() - *time < chrono::Duration::minutes(5) {
          queued.push(message);
        } else {
          *self = RemoteConnection::Offline;
        }
      }
      RemoteConnection::Offline => eprintln!("Ignoring message to offline server"),
    }
  }
}

impl Server {
  async fn attempt_remote_server_connection(self: &std::sync::Arc<Server>, remote_name: &str) -> () {
    let mut remote_contacts = self.attempting_remote_contacts.lock().await;
    if !remote_contacts.contains(remote_name)
      || match self.remotes.read().await.get(remote_name) {
        None => true,
        Some(key) => match self.remote_states.read().await.get(key.clone()) {
          None => true,
          Some(state) => match &*state.connection.lock().await {
            RemoteConnection::Online(_) => false,
            RemoteConnection::Dead(_, _) => true,
            RemoteConnection::Offline => true,
          },
        },
      }
    {
      remote_contacts.insert(remote_name.to_string());
      match remote_name.parse::<http::uri::Authority>() {
        Err(e) => {
          println!("Bad remote server name {}: {}", remote_name, e);
        }
        Ok(authority) => match hyper::Uri::builder().scheme(http::uri::Scheme::HTTPS).path_and_query("/api/server/v1").authority(authority).build() {
          Err(e) => {
            println!("Bad URL construction for server name {}: {}", remote_name, e);
          }
          Ok(uri) => match jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &RemoteClaim { exp: jwt_expiry_time(), name: self.name.clone() },
            &self.jwt_encoding_key,
          ) {
            Ok(token) => {
              let request = serde_json::to_vec(&RemoteConnectRequest { token, server: self.name.to_string() }).unwrap();
              let server_name = remote_name.to_string();
              tokio::spawn(async move {
                let mut delay = 30;
                loop {
                  let connector = hyper_tls::HttpsConnector::new();
                  let client = hyper::client::Client::builder().build::<_, hyper::Body>(connector);

                  match client
                    .request(hyper::Request::post(&uri).version(http::Version::HTTP_11).body(hyper::Body::from(request.clone())).unwrap())
                    .await
                  {
                    Err(e) => {
                      eprintln!("Failed contact to {}: {}", &server_name, e)
                    }
                    Ok(response) => {
                      if response.status() == http::StatusCode::OK {
                        return;
                      } else {
                        eprintln!("Failed to connect to remote server {}: {}", &server_name, response.status());
                        tokio::time::sleep(tokio::time::Duration::from_secs(delay)).await;
                        delay = 500.min(delay + 15);
                      }
                    }
                  }
                }
              });
            }
            Err(e) => {
              BAD_JWT.inc();
              eprintln!("Error generation JWT: {}", e);
            }
          },
        },
      }
    }
  }
  async fn check_player_state(
    self: &std::sync::Arc<Server>,
    target_player: &str,
    requesting_player: &str,
    requesting_server: Option<&str>,
  ) -> puzzleverse_core::PlayerLocationState {
    match self.load_player(target_player, false, |_| None).await {
      Some((target_player_id, _)) => match self.player_states.read().await.get(target_player_id) {
        Some(target_state) => {
          let online_acl = target_state.online_acl.lock().await;
          if online_acl.0.check(online_acl.1.iter(), requesting_player, &requesting_server, &self.name) {
            let target_mutable_state = target_state.mutable.lock().await;
            match &target_mutable_state.connection {
              player_state::PlayerConnection::Local(_, _, _) => {
                let location_acl = target_state.location_acl.lock().await;
                if location_acl.0.check(location_acl.1.iter(), requesting_player, &requesting_server, &self.name) {
                  match &target_mutable_state.goal {
                    player_state::Goal::InRealm(realm_key, _) => match self.realm_states.read().await.get(realm_key.clone()) {
                      Some(realm_state) => puzzleverse_core::PlayerLocationState::Realm(realm_state.id.clone(), self.name.clone()),
                      None => puzzleverse_core::PlayerLocationState::Online,
                    },
                    player_state::Goal::OnRemote(remote_id, realm_id) => match realm_id {
                      Some(realm) => match self.remote_states.read().await.get(remote_id.clone()) {
                        None => puzzleverse_core::PlayerLocationState::Online,
                        Some(remote_state) => puzzleverse_core::PlayerLocationState::Realm(realm.clone(), remote_state.name.clone()),
                      },
                      None => puzzleverse_core::PlayerLocationState::Online,
                    },
                    _ => puzzleverse_core::PlayerLocationState::InTransit,
                  }
                } else {
                  puzzleverse_core::PlayerLocationState::Online
                }
              }
              _ => puzzleverse_core::PlayerLocationState::Offline,
            }
          } else {
            puzzleverse_core::PlayerLocationState::Unknown
          }
        }
        None => puzzleverse_core::PlayerLocationState::Unknown,
      },
      None => puzzleverse_core::PlayerLocationState::Invalid,
    }
  }

  fn create_realm(
    db_connection: &r2d2::PooledConnection<diesel::r2d2::ConnectionManager<diesel::pg::PgConnection>>,
    asset: &str,
    owner: &str,
    name: Option<String>,
    seed: Option<i32>,
  ) -> diesel::QueryResult<String> {
    use sha3::Digest;
    let mut principal_hash = sha3::Sha3_512::new();
    principal_hash.update(owner.as_bytes());
    principal_hash.update(&[0]);
    principal_hash.update(asset.as_bytes());
    principal_hash.update(&[0]);
    principal_hash.update(chrono::Utc::now().to_rfc3339().as_bytes());
    let principal = hex::encode(principal_hash.finalize());
    use crate::schema::player::dsl as player_schema;
    use crate::schema::realm::dsl as realm_schema;
    diesel::insert_into(realm_schema::realm)
      .values((
        realm_schema::principal.eq(&principal),
        realm_schema::name.eq(name.unwrap_or(format!("{}'s {}", owner, asset))),
        realm_schema::owner
          .eq(sql_not_null_int(player_schema::player.select(player_schema::id).filter(player_schema::name.eq(owner)).single_value())),
        realm_schema::asset.eq(asset),
        realm_schema::state.eq(vec![]),
        realm_schema::seed.eq(seed.unwrap_or(rand::thread_rng().gen())),
        realm_schema::access_acl.eq(sql_not_null_bytes(
          player_schema::player.select(player_schema::new_realm_access_acl).filter(player_schema::name.eq(&owner)).single_value(),
        )),
        realm_schema::admin_acl.eq(sql_not_null_bytes(
          player_schema::player.select(player_schema::new_realm_access_acl).filter(player_schema::name.eq(&owner)).single_value(),
        )),
        realm_schema::in_directory.eq(false),
        realm_schema::initialised.eq(false),
      ))
      .returning(realm_schema::principal)
      .on_conflict_do_nothing()
      .get_result::<String>(db_connection)
  }
  async fn debut(self: &std::sync::Arc<Server>, player: &str) {
    let db_connection = self.db_pool.get().unwrap();
    use crate::schema::player::dsl as player_schema;
    diesel::update(player_schema::player.filter(player_schema::name.eq(player)))
      .set(player_schema::debuted.eq(true))
      .execute(&db_connection)
      .unwrap();
    if let Some(player_key) = self.players.read().await.get(player) {
      if let Some(player_state) = self.player_states.read().await.get(player_key.clone()) {
        player_state.debuted.store(true, std::sync::atomic::Ordering::Relaxed);
      }
    }
  }
  async fn find_asset(self: &std::sync::Arc<Server>, id: &str, post_action: AssetPullAction) -> bool {
    let mut outstanding_assets = self.outstanding_assets.lock().await;
    if self.asset_store.check(id) {
      true
    } else {
      match outstanding_assets.entry(id.to_string()) {
        std::collections::hash_map::Entry::Vacant(e) => {
          e.insert(vec![post_action]);
          let s = std::sync::Arc::downgrade(self);
          let id = id.to_string();
          tokio::spawn(async move {
            for attempt in 1..5 {
              use rand::seq::SliceRandom;
              let mut remotes: Vec<_> = match s.upgrade() {
                Some(server) => server.remote_states.read().await.keys().into_iter().collect(),
                None => return,
              };
              remotes.shuffle(&mut rand::thread_rng());
              for remote_id in remotes {
                let server = match s.upgrade() {
                  Some(server) => server,
                  None => return,
                };
                let request_sent = match server.remote_states.read().await.get(remote_id) {
                  Some(remote_state) => {
                    let mut connection = remote_state.connection.lock().await;
                    if let RemoteConnection::Online(c) = &mut *connection {
                      if let Err(e) = c.send(RemoteMessage::AssetsPull { assets: vec![id.clone()] }).await {
                        eprintln!("Failed to communicate with remote: {}", e);
                        false
                      } else {
                        true
                      }
                    } else {
                      false
                    }
                  }
                  None => false,
                };
                if request_sent {
                  // TODO: we should pick a threshold based on data...
                  tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                }
              }
              eprintln!("Failed to Find asset {} on attempt {}. Sleeping for a bit and will try again", &id, attempt);
              tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
            }
          });
        }
        std::collections::hash_map::Entry::Occupied(mut e) => {
          e.get_mut().push(post_action);
        }
      }
      false
    }
  }

  async fn find_or_create_realm<
    P: diesel::Expression<SqlType = diesel::sql_types::Bool>
      + diesel::expression::NonAggregate
      + diesel::expression::AppearsOnTable<schema::realm::table>
      + diesel::query_builder::QueryFragment<diesel::pg::Pg>
      + diesel::query_builder::QueryId,
  >(
    self: std::sync::Arc<Server>,
    player_id: PlayerKey,
    player_name: &str,
    player_server: Option<String>,
    create_asset: Option<(&'static str, String)>,
    predicate: P,
  ) -> (player_state::Goal, Option<puzzleverse_core::RealmChange>) {
    let mut realms = self.realms.lock().await;
    let candidate = {
      let db_connection = self.db_pool.get().unwrap();
      use crate::schema::realm::dsl as realm_schema;
      realm_schema::realm
        .select((realm_schema::principal, realm_schema::asset))
        .filter(predicate)
        .order_by(realm_schema::updated_at.desc())
        .get_result::<(String, String)>(&db_connection)
        .optional()
    };
    match candidate {
      Ok(None) => match create_asset {
        Some((asset, owner)) => {
          let result = {
            let db_connection = self.db_pool.get().unwrap();
            Server::create_realm(&db_connection, &asset, &owner, None, None)
          };
          match result {
            Err(e) => {
              eprintln!("Failed to create {} for {}: {}", &asset, &owner, e);
              (player_state::Goal::Undecided, Some(puzzleverse_core::RealmChange::Denied))
            }
            Ok(principal) => {
              let current = self.move_epoch.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
              let load = self
                .find_asset(&asset, AssetPullAction::LoadRealm(principal.clone(), std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(1))))
                .await;
              realms.insert(principal.clone(), RealmKind::WaitingAsset { asset: asset.to_string(), waiters: vec![(player_id, current)] });
              if load {
                self.clone().load_realm(principal).await;
              }
              (player_state::Goal::ResolvingLink(current), None)
            }
          }
        }
        None => (player_state::Goal::Undecided, Some(puzzleverse_core::RealmChange::Denied)),
      },
      Ok(Some((principal, asset))) => match realms.entry(principal.clone()) {
        std::collections::hash_map::Entry::Occupied(mut e) => match e.get_mut() {
          RealmKind::Loaded(key) => match self.realm_states.read().await.get(key.clone()) {
            None => (player_state::Goal::Undecided, Some(puzzleverse_core::RealmChange::Denied)),
            Some(realm_state) => {
              let allowed = {
                let access_acl = realm_state.access_acl.lock().await;
                access_acl.0.check(access_acl.1.iter(), player_name, &player_server, &self.name)
              };
              if allowed {
                let asset = realm_state.asset.clone();
                if let Err(e) = self.push_assets.lock().await.send((player_id.clone(), asset)).await {
                  eprintln!("Failed to start asset push process: {}", e);
                }
                (
                  crate::player_state::Goal::WaitingAssetTransfer(key.clone()),
                  Some(puzzleverse_core::RealmChange::Success {
                    realm: principal,
                    server: self.name.clone(),
                    name: realm_state.name.read().await.clone(),
                    asset: realm_state.asset.clone(),
                    seed: realm_state.seed,
                    settings: realm_state.puzzle_state.lock().await.settings.clone(),
                  }),
                )
              } else {
                (crate::player_state::Goal::Undecided, Some(puzzleverse_core::RealmChange::Denied))
              }
            }
          },
          RealmKind::WaitingAsset { waiters, .. } => {
            let current = self.move_epoch.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            waiters.push((player_id, current));
            (player_state::Goal::ResolvingLink(current), None)
          }
        },
        std::collections::hash_map::Entry::Vacant(v) => {
          let current = self.move_epoch.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
          let load =
            self.find_asset(&asset, AssetPullAction::LoadRealm(principal.clone(), std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(1)))).await;
          v.insert(RealmKind::WaitingAsset { asset, waiters: vec![(player_id, current)] });
          if load {
            self.clone().load_realm(principal).await;
          }
          (player_state::Goal::ResolvingLink(current), None)
        }
      },
      Err(e) => {
        eprintln!("Failed to get realm: {}", e);
        (player_state::Goal::Undecided, Some(puzzleverse_core::RealmChange::Denied))
      }
    }
  }
  /// Deal with an incoming WebSocket from a player
  ///
  /// Each player's client will have on active WebSocket used for messages. Messages are asynchronously communicated between client and server, so this splits the responsibility of processing incoming and outgoing messages.
  async fn handle_client_websocket(
    self: std::sync::Arc<Server>,
    player_claim: PlayerClaim,
    ws: tokio_tungstenite::WebSocketStream<hyper::upgrade::Upgraded>,
  ) {
    eprintln!("Connection for player: {}", &player_claim.name);
    let active = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let (read, player, db_id) = {
      let (write, read) = ws.split();
      let (player, db_id) = self
        .load_player(&player_claim.name, true, |id| {
          Some(crate::player_state::PlayerConnection::Local(id, write.with(crate::encode_message), active.clone()))
        })
        .await
        .unwrap();
      (read, player, db_id)
    };

    tokio::spawn(async move {
      read
        .for_each(|m| async {
          if active.load(std::sync::atomic::Ordering::Relaxed) {
            if let Ok(tokio_tungstenite::tungstenite::protocol::Message::Binary(buf)) = m {
              {
                match rmp_serde::from_slice::<puzzleverse_core::ClientRequest>(&buf) {
                  Ok(req) => {
                    if self.process_client_request(&player_claim.name, &player, db_id, req).await {
                      active.store(false, std::sync::atomic::Ordering::Relaxed)
                    }
                  }
                  Err(_) => BAD_CLIENT_REQUESTS.with_label_values(&[&player_claim.name]).inc(),
                }
              }
            }
          }
        })
        .await;
    });
  }
  /// Process an incoming HTTP request
  async fn handle_http_request(self: std::sync::Arc<Server>, req: http::Request<hyper::Body>) -> Result<http::Response<hyper::Body>, http::Error> {
    match (req.method(), req.uri().path()) {
      // For the root, provide an HTML PAGE with the web client
      (&http::Method::GET, "/") => http::Response::builder().header("Content-Type", "text/html; charset=utf-8").body(html::create_main().into()),
      (&http::Method::GET, "/metrics") => {
        use prometheus::Encoder;
        let encoder = prometheus::TextEncoder::new();
        let mut buffer = vec![];
        let mf = prometheus::gather();
        encoder.encode(&mf, &mut buffer).unwrap();
        http::Response::builder().header(hyper::header::CONTENT_TYPE, encoder.format_type()).body(buffer.into())
      }
      // Describe what authentication scheme is used
      (&http::Method::GET, "/api/auth/method") => {
        let scheme = self.authentication.scheme();
        match serde_json::to_string(&scheme) {
          Err(e) => {
            BAD_WEB_REQUEST.inc();
            eprintln!("Failed to serialise authentication scheme: {}", e);
            http::Response::builder().status(http::StatusCode::BAD_REQUEST).body(hyper::Body::empty())
          }
          Ok(auth_json) => http::Response::builder().status(http::StatusCode::OK).header("Content-Type", "application/json").body(auth_json.into()),
        }
      }
      // Deliver the webclient
      #[cfg(feature = "wasm-client")]
      (&http::Method::GET, "/puzzleverse-client_bg.wasm") => etag_request("application/wasm", include_bytes!("../puzzleverse-client_bg.wasm"), req),
      #[cfg(feature = "wasm-client")]
      (&http::Method::GET, "/puzzleverse-client.js") => etag_request("text/javascript", include_bytes!("../puzzleverse-client.js"), req),
      // Handle a new player connection by upgrading to a web socket
      (&http::Method::GET, "/api/client/v1") => self.open_websocket(req, Server::handle_client_websocket),
      // Handle a new server connection by upgrading to a web socket
      (&http::Method::GET, "/api/server/v1") => self.open_websocket(req, Server::handle_server_websocket),
      (&http::Method::GET, "/api/client/key") => http::Response::builder().status(http::StatusCode::OK).body(self.signing_key(0).into()),
      (&http::Method::POST, "/api/client/key") => match hyper::body::aggregate(req).await {
        Err(e) => {
          BAD_WEB_REQUEST.inc();
          eprintln!("Failed to aggregate body: {}", e);
          http::Response::builder().status(http::StatusCode::INTERNAL_SERVER_ERROR).body(format!("Aggregation failed: {}", e).into())
        }
        Ok(whole_body) => {
          use bytes::buf::Buf;
          match serde_json::from_reader::<_, puzzleverse_core::AuthPublicKey>(whole_body.reader()) {
            Err(e) => http::Response::builder().status(http::StatusCode::BAD_REQUEST).body(e.to_string().into()),
            Ok(data) => {
              use schema::player::dsl as player_dsl;
              use schema::publickey::dsl as publickey_dsl;
              let db_connection = self.db_pool.get().unwrap();
              match publickey_dsl::publickey
                .select(publickey_dsl::public_key)
                .filter(
                  publickey_dsl::name.eq(&data.name).and(
                    publickey_dsl::player
                      .eq(sql_not_null_int(player_dsl::player.select(player_dsl::id).filter(player_dsl::name.eq(&data.player)).single_value())),
                  ),
                )
                .load::<Vec<u8>>(&db_connection)
              {
                Ok(public_keys) => {
                  for der in public_keys {
                    if let Ok(pkey) = openssl::pkey::PKey::public_key_from_der(der.as_slice()) {
                      for delta in 0..1 {
                        if let Ok(mut verifier) = openssl::sign::Verifier::new(openssl::hash::MessageDigest::sha256(), &pkey) {
                          if let Err(e) = verifier.update(self.signing_key(delta).as_bytes()) {
                            eprintln!("Signature verification error: {}", e);
                            continue;
                          }
                          if verifier.verify(&data.signature).unwrap_or(false) {
                            return match jsonwebtoken::encode(
                              &jsonwebtoken::Header::default(),
                              &PlayerClaim { exp: jwt_expiry_time(), name: data.player },
                              &self.jwt_encoding_key,
                            ) {
                              Ok(token) => http::Response::builder().status(http::StatusCode::OK).body(token.into()),
                              Err(e) => {
                                BAD_WEB_REQUEST.inc();
                                eprintln!("Error generation JWT: {}", e);
                                http::Response::builder().status(http::StatusCode::INTERNAL_SERVER_ERROR).body("Failed to generate token".into())
                              }
                            };
                          }
                        }
                      }
                    }
                  }
                }
                Err(e) => {
                  eprintln!("Failed to fetch public keys during authentication: {}", e);
                }
              }
              http::Response::builder().status(http::StatusCode::FORBIDDEN).body("No matching key".into())
            }
          }
        }
      },
      // Handle a request by a remote server for a connection back
      (&http::Method::POST, "/api/server/v1") => match hyper::body::aggregate(req).await {
        Err(e) => {
          BAD_WEB_REQUEST.inc();
          eprintln!("Failed to aggregate body: {}", e);
          http::Response::builder().status(http::StatusCode::INTERNAL_SERVER_ERROR).body(format!("Aggregation failed: {}", e).into())
        }
        Ok(whole_body) => {
          use bytes::buf::Buf;
          match serde_json::from_reader::<_, RemoteConnectRequest>(whole_body.reader()) {
            Err(e) => http::Response::builder().status(http::StatusCode::BAD_REQUEST).body(e.to_string().into()),
            Ok(mut data) => {
              data.server.make_ascii_lowercase();
              //TODO: we should be able to block remote servers
              match data.server.parse::<http::uri::Authority>() {
                Ok(authority) => {
                  match hyper::Uri::builder().scheme(http::uri::Scheme::HTTPS).path_and_query("/api/server/v1").authority(authority).build() {
                    Ok(uri) => {
                      let server = self.clone();
                      tokio::spawn(async move {
                        let connector = hyper_tls::HttpsConnector::new();
                        let client = hyper::client::Client::builder().build::<_, hyper::Body>(connector);

                        match client
                          .request(
                            hyper::Request::get(uri)
                              .version(http::Version::HTTP_11)
                              .header(http::header::CONNECTION, "upgrade")
                              .header(http::header::SEC_WEBSOCKET_VERSION, "13")
                              .header(http::header::SEC_WEBSOCKET_PROTOCOL, "puzzleverse")
                              .header(http::header::UPGRADE, "websocket")
                              .header(http::header::SEC_WEBSOCKET_KEY, format!("pv{}", &mut rand::thread_rng().next_u64()))
                              .header(http::header::AUTHORIZATION, format!("Bearer {}", data.token))
                              .body(hyper::Body::empty())
                              .unwrap(),
                          )
                          .await
                        {
                          Err(e) => {
                            FAILED_SERVER_CALLBACK.with_label_values(&[&data.server]).inc();
                            eprintln!("Failed callback to {}: {}", data.server, e)
                          }
                          Ok(response) => {
                            if response.status() == http::StatusCode::SWITCHING_PROTOCOLS {
                              match hyper::upgrade::on(response).await {
                                Ok(upgraded) => {
                                  server
                                    .handle_server_websocket(
                                      RemoteClaim { exp: jwt_expiry_time(), name: data.server },
                                      tokio_tungstenite::WebSocketStream::from_raw_socket(
                                        upgraded,
                                        tokio_tungstenite::tungstenite::protocol::Role::Client,
                                        None,
                                      )
                                      .await,
                                    )
                                    .await
                                }
                                Err(e) => {
                                  FAILED_SERVER_CALLBACK.with_label_values(&[&data.server]).inc();
                                  eprintln!("Failed to connect to {}: {}", data.server, e);
                                }
                              }
                            } else {
                              FAILED_SERVER_CALLBACK.with_label_values(&[&data.server]).inc();
                              let status = response.status();
                              match hyper::body::aggregate(response).await {
                                Err(e) => eprintln!("Failed to connect to {} {}: {}", data.server, status, e),
                                Ok(buf) => eprintln!(
                                  "Failed to connect to {} {}: {}",
                                  data.server,
                                  status,
                                  std::str::from_utf8(buf.chunk()).unwrap_or("Bad UTF-8 data"),
                                ),
                              }
                            }
                          }
                        }
                      });
                      http::Response::builder().status(http::StatusCode::OK).body("Will do".into())
                    }
                    Err(e) => {
                      FAILED_SERVER_CALLBACK.with_label_values(&[&data.server]).inc();
                      http::Response::builder().status(http::StatusCode::BAD_REQUEST).body(format!("{}", e).into())
                    }
                  }
                }
                Err(e) => {
                  FAILED_SERVER_CALLBACK.with_label_values(&[&data.server]).inc();
                  http::Response::builder().status(http::StatusCode::BAD_REQUEST).body(format!("{}", e).into())
                }
              }
            }
          }
        }
      },
      // For other URLs, see if the authentication mechanism is prepared to deal with them
      _ => match self.authentication.handle(req).await {
        auth::AuthResult::Failure => {
          http::Response::builder().status(http::StatusCode::INTERNAL_SERVER_ERROR).body("Internal server error during authentication".into())
        }
        auth::AuthResult::NotHandled => http::Response::builder().status(http::StatusCode::NOT_FOUND).body("Not Found".into()),
        auth::AuthResult::Page(page) => page,
        auth::AuthResult::SendToken(user_name) => {
          match jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &PlayerClaim { exp: jwt_expiry_time(), name: user_name },
            &self.jwt_encoding_key,
          ) {
            Ok(token) => http::Response::builder().status(http::StatusCode::OK).body(token.into()),
            Err(e) => {
              BAD_WEB_REQUEST.inc();
              eprintln!("Error generation JWT: {}", e);
              http::Response::builder().status(http::StatusCode::INTERNAL_SERVER_ERROR).body("Failed to generate token".into())
            }
          }
        }
      },
    }
  }
  /// Deal with an incoming WebSocket from a federated server
  ///
  /// Each player's client will have on active WebSocket used for messages. Messages are asynchronously communicated between client and server, so this splits the responsibility of processing incoming and outgoing messages.
  async fn handle_server_websocket(
    self: std::sync::Arc<Server>,
    server_claim: RemoteClaim,
    ws: tokio_tungstenite::WebSocketStream<hyper::upgrade::Upgraded>,
  ) {
    let server_for_reader = self.clone();

    let (read, (remote_id, dead)) = {
      let mut remotes = (*self).remotes.write().await;
      let (write, read) = ws.split();

      let mut new_state = RemoteConnection::Online(write.with(crate::encode_message));
      let queued_messages = {
        let db_connection = self.db_pool.get().unwrap();
        use crate::schema::player::dsl as player_schema;
        use crate::schema::remoteplayerchat::dsl as chat_schema;
        chat_schema::remoteplayerchat
          .select((
            chat_schema::body,
            chat_schema::remote_player,
            chat_schema::created,
            sql_not_null_str(player_schema::player.select(player_schema::name).filter(player_schema::id.eq(chat_schema::player)).single_value()),
          ))
          .filter(chat_schema::remote_server.eq(&server_claim.name).and(chat_schema::state.eq("O")))
          .load::<(String, String, chrono::DateTime<chrono::Utc>, String)>(&db_connection)
      };
      match queued_messages {
        Ok(mut messages) => {
          new_state
            .send(RemoteMessage::DirectMessage(
              messages.drain(..).map(|(body, recipient, timestamp, sender)| RemoteDirectMessage { sender, recipient, timestamp, body }).collect(),
            ))
            .await;
          let db_connection = self.db_pool.get().unwrap();
          use crate::schema::remoteplayerchat::dsl as chat_schema;
          if let Err(e) =
            diesel::update(chat_schema::remoteplayerchat.filter(chat_schema::remote_server.eq(&server_claim.name).and(chat_schema::state.eq("O"))))
              .set(chat_schema::state.eq("o"))
              .execute(&db_connection)
          {
            eprintln!("Failed to marked queued messages as sent for {}: {}", &server_claim.name, e)
          }
        }
        Err(e) => eprintln!("Failed to pull queued messages for {}: {}", &server_claim.name, e),
      }

      let mut active_states = (*self).remote_states.write().await;
      (
        read,
        match remotes.entry(server_claim.name.clone()) {
          std::collections::hash_map::Entry::Vacant(entry) => {
            let remote_id = active_states.insert(RemoteState {
              connection: tokio::sync::Mutex::new(new_state),
              interested_in_list: tokio::sync::Mutex::new(std::collections::HashSet::new()),
              name: server_claim.name.clone(),
            });
            entry.insert(remote_id.clone());
            (remote_id, None)
          }
          std::collections::hash_map::Entry::Occupied(mut entry) => match active_states.get_mut(*entry.get()) {
            None => {
              let remote_id = active_states.insert(RemoteState {
                connection: tokio::sync::Mutex::new(new_state),
                interested_in_list: tokio::sync::Mutex::new(std::collections::HashSet::new()),
                name: server_claim.name.clone(),
              });
              entry.insert(remote_id.clone());
              (remote_id, None)
            }
            Some(value) => {
              let mut current = value.connection.lock().await;
              std::mem::swap(&mut new_state, &mut *current);
              (
                entry.get().clone(),
                match new_state {
                  RemoteConnection::Online(socket) => Some(socket),
                  RemoteConnection::Dead(_, mut queued) => {
                    for message in queued.drain(..) {
                      current.send(message).await;
                    }
                    None
                  }
                  RemoteConnection::Offline => None,
                },
              )
            }
          },
        },
      )
    };
    self.attempting_remote_contacts.lock().await.remove(&server_claim.name);
    if let Some(mut dead_socket) = dead {
      if let Err(e) = dead_socket.close().await {
        FAILED_SERVER_EVICT.with_label_values(&[&server_claim.name]).inc();
        eprintln!("Error evicting old connection for server {}: {}", server_claim.name, e)
      }
    }

    tokio::spawn(async move {
      let s = server_for_reader.clone();
      read
        .for_each(|m| async {
          match m {
            Ok(tokio_tungstenite::tungstenite::protocol::Message::Binary(buf)) => match rmp_serde::from_slice::<RemoteMessage>(&buf) {
              Ok(req) => s.process_server_message(&server_claim.name, &remote_id, req).await,
              Err(err) => {
                BAD_SERVER_REQUESTS.with_label_values(&[&server_claim.name]).inc();
                eprintln!("Error from {}: {}", server_claim.name, err)
              }
            },
            Ok(tokio_tungstenite::tungstenite::protocol::Message::Ping(_)) => {
              eprintln!("Ping from {}", server_claim.name)
            }
            Ok(tokio_tungstenite::tungstenite::protocol::Message::Pong(_)) => {
              eprintln!("Ping from {}", server_claim.name)
            }
            Ok(tokio_tungstenite::tungstenite::protocol::Message::Text(message)) => {
              eprintln!("Text from {}: {}", server_claim.name, message)
            }
            Ok(tokio_tungstenite::tungstenite::protocol::Message::Close(_)) => {
              eprintln!("Connection from {} is closing", server_claim.name)
            }
            Err(err) => {
              BAD_SERVER_REQUESTS.with_label_values(&[&server_claim.name]).inc();
              eprintln!("Error from {}: {}", server_claim.name, err)
            }
          }
        })
        .await;
    });
  }
  fn list_realms<
    P: diesel::Expression<SqlType = diesel::sql_types::Bool>
      + diesel::expression::NonAggregate
      + diesel::expression::AppearsOnTable<schema::realm::table>
      + diesel::query_builder::QueryFragment<diesel::pg::Pg>
      + diesel::query_builder::QueryId,
  >(
    &self,
    predicate: P,
  ) -> Vec<puzzleverse_core::Realm> {
    let db_connection = self.db_pool.get().unwrap();
    use crate::schema::realm::dsl as realm_schema;
    match realm_schema::realm.select((realm_schema::principal, realm_schema::name)).filter(predicate).load::<(String, String)>(&db_connection) {
      Ok(mut entries) => entries.drain(..).map(|(id, name)| puzzleverse_core::Realm { id, name }).collect(),
      Err(e) => {
        eprintln!("Failed to get realms from DB: {}", e);
        vec![]
      }
    }
  }
  async fn load_player<F: FnOnce(i32) -> Option<player_state::PlayerConnection>>(
    self: &std::sync::Arc<Server>,
    player_name: &str,
    create: bool,
    create_connection: F,
  ) -> Option<(PlayerKey, i32)> {
    let db_info = {
      use crate::schema::player::dsl as player_schema;
      let db_connection = &*self.db_pool.clone().get().unwrap();
      db_connection
        .transaction::<_, diesel::result::Error, _>(|| {
          // Find or create the player's database entry
          let mut results: Vec<(i32, bool, Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>)> = player_schema::player
            .select((
              player_schema::id,
              player_schema::debuted,
              player_schema::message_acl,
              player_schema::online_acl,
              player_schema::location_acl,
              player_schema::new_realm_access_acl,
              player_schema::new_realm_admin_acl,
            ))
            .filter(player_schema::name.eq(&player_name))
            .load(db_connection)?;
          let db_record = match results.drain(..).next() {
            None => {
              if create {
                diesel::insert_into(player_schema::player)
                  .values((
                    player_schema::name.eq(&player_name),
                    player_schema::debuted.eq(false),
                    player_schema::message_acl.eq(vec![]),
                    player_schema::online_acl.eq(vec![]),
                    player_schema::location_acl.eq(vec![]),
                    player_schema::new_realm_access_acl.eq(vec![]),
                    player_schema::new_realm_admin_acl.eq(vec![]),
                  ))
                  .returning((
                    player_schema::id,
                    player_schema::debuted,
                    player_schema::message_acl,
                    player_schema::online_acl,
                    player_schema::location_acl,
                    player_schema::new_realm_access_acl,
                    player_schema::new_realm_admin_acl,
                  ))
                  .get_result::<(i32, bool, Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>)>(db_connection)
                  .optional()?
              } else {
                None
              }
            }
            Some(record) => Some(record),
          };

          Ok(match db_record {
            Some((id, debuted, message_acl, online_acl, location_acl, access_acl, admin_acl)) => {
              diesel::update(player_schema::player.filter(player_schema::id.eq(id)))
                .set(player_schema::last_login.eq(chrono::Utc::now()))
                .execute(db_connection)?;

              Some((
                id,
                debuted,
                Server::parse_player_acl(
                  message_acl,
                  (puzzleverse_core::AccessDefault::Deny, vec![puzzleverse_core::AccessControl::AllowLocal(None)]),
                ),
                Server::parse_player_acl(online_acl, (puzzleverse_core::AccessDefault::Deny, vec![])),
                Server::parse_player_acl(location_acl, (puzzleverse_core::AccessDefault::Deny, vec![])),
                Server::parse_player_acl(access_acl, (puzzleverse_core::AccessDefault::Deny, vec![])),
                Server::parse_player_acl(admin_acl, (puzzleverse_core::AccessDefault::Deny, vec![])),
              ))
            }
            None => None,
          })
        })
        .unwrap()
    };
    fn new_state(
      player_name: &str,
      server: &Server,
      debuted: bool,
      message_acl: std::sync::Arc<tokio::sync::Mutex<AccessControlSetting>>,
      online_acl: std::sync::Arc<tokio::sync::Mutex<AccessControlSetting>>,
      location_acl: std::sync::Arc<tokio::sync::Mutex<AccessControlSetting>>,
      new_realm_access_acl: std::sync::Arc<tokio::sync::Mutex<AccessControlSetting>>,
      new_realm_admin_acl: std::sync::Arc<tokio::sync::Mutex<AccessControlSetting>>,
      new_connection: crate::player_state::PlayerConnection,
    ) -> crate::player_state::PlayerState {
      crate::player_state::PlayerState {
        debuted: debuted.into(),
        name: player_name.to_string(),
        principal: format!("{}@{}", player_name, server.name),
        server: Some(server.name.clone()),
        mutable: tokio::sync::Mutex::new(crate::player_state::MutablePlayerState { connection: new_connection, goal: player_state::Goal::Undecided }),
        message_acl,
        online_acl,
        location_acl,
        new_realm_access_acl,
        new_realm_admin_acl,
      }
    }
    match db_info {
      Some((db_id, debuted, message_acl, online_acl, location_acl, access_acl, admin_acl)) => {
        let mut active_players = self.player_states.write().await;
        let (player, old_socket) = match self.players.write().await.entry(player_name.to_string()) {
          std::collections::hash_map::Entry::Vacant(entry) => {
            let id = self.player_states.write().await.insert(new_state(
              &player_name,
              &self,
              debuted,
              message_acl,
              online_acl,
              location_acl,
              access_acl,
              admin_acl,
              create_connection(db_id).unwrap_or(player_state::PlayerConnection::Offline),
            ));

            entry.insert(id.clone());
            (id, None)
          }
          std::collections::hash_map::Entry::Occupied(mut entry) => (
            entry.get().clone(),
            match active_players.get_mut(*entry.get()) {
              None => {
                entry.insert(active_players.insert(new_state(
                  &player_name,
                  &self,
                  debuted,
                  message_acl,
                  online_acl,
                  location_acl,
                  access_acl,
                  admin_acl,
                  create_connection(db_id).unwrap_or(player_state::PlayerConnection::Offline),
                )));
                None
              }
              Some(value) => match create_connection(db_id) {
                None => None,
                Some(mut new_connection) => {
                  let mut old = value.mutable.lock().await;
                  std::mem::swap(&mut new_connection, &mut old.connection);
                  match new_connection {
                    crate::player_state::PlayerConnection::Local(_, socket, active) => {
                      active.store(false, std::sync::atomic::Ordering::Relaxed);
                      Some(socket)
                    }
                    crate::player_state::PlayerConnection::LocalDead(_, _, mut queued) => {
                      if let crate::player_state::Goal::InRealm(realm, _) = &old.goal {
                        if let Some(realm_state) = self.realm_states.read().await.get(realm.clone()) {
                          old
                            .connection
                            .send_change(
                              self,
                              puzzleverse_core::RealmChange::Success {
                                asset: realm_state.asset.clone(),
                                name: realm_state.name.read().await.clone(),
                                realm: realm_state.id.clone(),
                                seed: realm_state.seed,
                                server: self.name.clone(),
                                settings: realm_state.puzzle_state.lock().await.settings.clone(),
                              },
                            )
                            .await;
                        }
                      }
                      for message in queued.drain(..) {
                        old.connection.send_local(message).await;
                      }
                      None
                    }
                    _ => None,
                  }
                }
              },
            },
          ),
        };
        if let Some(mut old_socket) = old_socket {
          let pn = player_name.to_string();
          tokio::spawn(async move {
            match old_socket.send(puzzleverse_core::ClientResponse::Disconnect).await {
              Ok(_) => (),
              Err(e) => {
                eprintln!("Could not close replaced websocket: {}", e);
                FAILED_CLIENT_EVICT.with_label_values(&[&pn]).inc();
              }
            };
            if let Err(e) = old_socket.close().await {
              eprintln!("Error evicting old connection for player {}: {}", pn, e);
              FAILED_CLIENT_EVICT.with_label_values(&[&pn]).inc();
            }
          });
        }
        Some((player, db_id))
      }
      None => None,
    }
  }
  async fn load_realm(self: std::sync::Arc<Server>, principal: String) {
    tokio::spawn(async move {
      match realm::RealmState::load(&self, &principal).await {
        Ok(state) => {
          let (access_default, access_acls) = (*state.access_acl.lock().await).clone();
          let asset = state.asset.clone();
          let success = puzzleverse_core::RealmChange::Success {
            realm: principal.clone(),
            server: self.name.clone(),
            name: state.name.read().await.clone(),
            asset: asset.clone(),
            seed: state.seed,
            settings: state.puzzle_state.lock().await.settings.clone(),
          };
          let realm_key = self.realm_states.write().await.insert(state);
          match self.realms.lock().await.insert(principal, RealmKind::Loaded(realm_key.clone())) {
            None => eprintln!("Loaded a realm no was interested in"),
            Some(RealmKind::Loaded(_)) => {
              panic!("Replaced an active realm. This should not happen.")
            }
            Some(RealmKind::WaitingAsset { mut waiters, .. }) => {
              let player_states = self.player_states.read().await;
              for (player, epoch) in waiters.drain(..) {
                if let Some(player_state) = player_states.get(player.clone()) {
                  let mut mutable_player_state = player_state.mutable.lock().await;
                  if mutable_player_state.goal == player_state::Goal::ResolvingLink(epoch) {
                    let (new_goal, change) = if access_default.check(access_acls.iter(), &player_state.name, &player_state.server, &self.name) {
                      if let Err(e) = self.push_assets.lock().await.send((player.clone(), asset.clone())).await {
                        eprintln!("Failed to start asset push process: {}", e);
                      }
                      (crate::player_state::Goal::WaitingAssetTransfer(realm_key.clone()), success.clone())
                    } else {
                      mutable_player_state.connection.release_player(&self).await;
                      (crate::player_state::Goal::Undecided, puzzleverse_core::RealmChange::Denied)
                    };
                    mutable_player_state.goal = new_goal;
                    mutable_player_state.connection.send_change(&self, change).await;
                  }
                }
              }
            }
          }
        }
        Err(e) => {
          eprintln!("Failed to load realm: {:?}", e);
        }
      }
    });
  }
  async fn load_realm_description<
    T,
    E,
    F: FnOnce(
      Vec<Box<dyn crate::puzzle::PuzzleAsset>>,
      Vec<crate::puzzle::PropagationRule>,
      Vec<crate::puzzle::ConsequenceRule>,
      crate::realm::navigation::RealmManifold,
      std::collections::BTreeMap<String, puzzleverse_core::RealmSetting>,
    ) -> Result<T, E>,
  >(
    &self,
    asset: &str,
    func: F,
  ) -> Result<T, E> {
    todo!()
  }
  async fn move_player(self: &std::sync::Arc<Server>, request: RealmMove) -> () {
    let (player_id, release_target) = match request {
      RealmMove::ToHome(id) => (id, ReleaseTarget::Home),
      RealmMove::ToExistingRealm { player, server: remote_name, realm } => (
        player,
        ReleaseTarget::Realm(
          realm,
          match remote_name {
            Some(name) => name,
            None => self.name.clone(),
          },
        ),
      ),
      RealmMove::ToRealm { player, owner, asset } => {
        let db_connection = self.db_pool.get().unwrap();
        (
          player,
          match db_connection.transaction::<_, diesel::result::Error, _>(|| {
            use crate::schema::player::dsl as player_schema;
            use crate::schema::realm::dsl as realm_schema;
            let result = realm_schema::realm
              .select(realm_schema::principal)
              .filter(
                realm_schema::asset.eq(&asset).and(
                  realm_schema::owner
                    .nullable()
                    .eq(player_schema::player.select(player_schema::id).filter(player_schema::name.eq(&owner)).single_value()),
                ),
              )
              .get_result::<String>(&db_connection)
              .optional()?;

            match result {
              Some(id) => Ok(ReleaseTarget::Realm(id, self.name.clone())),
              None => Ok(ReleaseTarget::Realm(Server::create_realm(&db_connection, &asset, &owner, None, None)?, self.name.clone())),
            }
          }) {
            Err(e) => {
              eprintln!("Failed to figure out realm {} for {}: {}", &asset, &owner, e);
              ReleaseTarget::Transit
            }
            Ok(v) => v,
          },
        )
      }
    };
    match self.player_states.read().await.get(player_id.clone()) {
      None => (),
      Some(state) => {
        let mut mutable_state = state.mutable.lock().await;
        let mut set_dead = None;
        let mut new_goal = None;
        match &mut mutable_state.connection {
          player_state::PlayerConnection::Offline => (),
          player_state::PlayerConnection::Local(db_id, connection, _) => {
            new_goal = Some(self.resolve_realm(&player_id, &state.name, None, *db_id, release_target).await);
            if let Err(e) = connection.send(puzzleverse_core::ClientResponse::InTransit).await {
              eprintln!("Failed to handoff player: {}", e);
              set_dead = Some(*db_id);
            }
          }
          player_state::PlayerConnection::LocalDead(db_id, _, queue) => {
            new_goal = Some(self.resolve_realm(&player_id, &state.name, None, *db_id, release_target).await);
            queue.push(puzzleverse_core::ClientResponse::InTransit);
          }
          player_state::PlayerConnection::Remote(player, remote_id) => match self.remote_states.read().await.get(remote_id.clone()) {
            None => {
              eprintln!("Trying to hand off player {} to missing sever.", &player)
            }
            Some(remote_state) => {
              remote_state.connection.lock().await.send(RemoteMessage::VisitorRelease(player.clone(), release_target)).await;
              mutable_state.goal = player_state::Goal::OnRemote(remote_id.clone(), None);
            }
          },
        };
        if let Some((goal, change)) = new_goal {
          mutable_state.goal = goal;
          if let Some(change) = change {
            mutable_state.connection.send_change(self, change).await;
          }
        }
        if let Some(db_id) = set_dead {
          mutable_state.connection =
            player_state::PlayerConnection::LocalDead(db_id, chrono::Utc::now(), vec![puzzleverse_core::ClientResponse::InTransit]);
        }
      }
    }
  }
  async fn move_players_from_realm<T: IntoIterator<Item = (crate::PlayerKey, crate::puzzle::RealmLink)>>(&self, realm_owner: &str, links: T) {
    let queue = self.move_queue.lock().await;
    for (player, link) in links {
      match link {
        puzzle::RealmLink::Global(realm, server_name) => {
          if let Err(e) =
            queue.send(RealmMove::ToExistingRealm { player, realm, server: if &server_name == &self.name { None } else { Some(server_name) } }).await
          {
            eprintln!("Failed to put player into move queue: {}", e);
          }
        }
        puzzle::RealmLink::Owner(asset) => {
          if let Err(e) = queue.send(RealmMove::ToRealm { player, owner: realm_owner.to_string(), asset }).await {
            eprintln!("Failed to put player into move queue: {}", e);
          }
        }
        puzzle::RealmLink::Spawn(_) => eprintln!("Trying to move player to spawn point. This should have been dealt with already"),
        puzzle::RealmLink::Home => {
          if let Err(e) = queue.send(RealmMove::ToHome(player)).await {
            eprintln!("Failed to put player into move queue: {}", e);
          }
        }
      }
    }
  }
  fn open_websocket<
    R: 'static + Future<Output = ()> + Send,
    F: 'static + Send + FnOnce(std::sync::Arc<Server>, T, tokio_tungstenite::WebSocketStream<hyper::upgrade::Upgraded>) -> R,
    T: 'static + serde::de::DeserializeOwned + Send,
  >(
    self: std::sync::Arc<Server>,
    req: hyper::Request<hyper::Body>,
    handler: F,
  ) -> Result<hyper::Response<hyper::Body>, http::Error> {
    // Check whether they provided a valid Authorization: Bearer header or token= URL parameter
    match req
      .headers()
      .get(http::header::AUTHORIZATION)
      .map(|h| match h.to_str() {
        Ok(value) => Some(std::borrow::Cow::Borrowed(value)),
        Err(_) => None,
      })
      .flatten()
      .or_else(|| {
        req.uri().query().map(|q| form_urlencoded::parse(q.as_bytes()).filter(|(name, _)| name == "token").map(|(_, value)| value).next()).flatten()
      })
      .map(|value| {
        if value.starts_with("Bearer ") {
          match jsonwebtoken::decode::<T>(&value[7..], &(*self).jwt_decoding_key, &jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::HS256)) {
            Err(e) => {
              BAD_JWT.inc();
              eprintln!("JWT decoding failure: {}", e);
              None
            }
            Ok(data) => Some(data.claims),
          }
        } else {
          None
        }
      })
      .flatten()
    {
      Some(token_contents) => {
        let is_http_11 = req.version() == http::Version::HTTP_11;
        let is_upgrade = req.headers().get(http::header::CONNECTION).map_or(false, |v| connection_has(v, "upgrade"));
        let is_websocket_upgrade =
          req.headers().get(http::header::UPGRADE).and_then(|v| v.to_str().ok()).map_or(false, |v| v.eq_ignore_ascii_case("websocket"));
        let is_websocket_version_13 =
          req.headers().get(http::header::SEC_WEBSOCKET_VERSION).and_then(|v| v.to_str().ok()).map_or(false, |v| v == "13");
        if !is_http_11 || !is_upgrade || !is_websocket_upgrade || !is_websocket_version_13 {
          BAD_WEB_REQUEST.inc();
          return http::Response::builder()
            .status(http::StatusCode::UPGRADE_REQUIRED)
            .header(http::header::SEC_WEBSOCKET_VERSION, "13")
            .body("Expected Upgrade to WebSocket version 13".into());
        }
        match req.headers().get(http::header::SEC_WEBSOCKET_KEY) {
          Some(value) => {
            let accept = convert_key(value.as_bytes());
            tokio::spawn(async move {
              match hyper::upgrade::on(req).await {
                Err(e) => {
                  BAD_WEB_REQUEST.inc();
                  eprintln!("Upgrade error: {}", e);
                }
                Ok(upgraded) => {
                  handler(
                    self,
                    token_contents,
                    tokio_tungstenite::WebSocketStream::from_raw_socket(upgraded, tokio_tungstenite::tungstenite::protocol::Role::Server, None).await,
                  )
                  .await
                }
              }
            });

            http::Response::builder()
              .status(http::StatusCode::SWITCHING_PROTOCOLS)
              .header(http::header::UPGRADE, "websocket")
              .header(http::header::CONNECTION, "upgrade")
              .header(http::header::SEC_WEBSOCKET_ACCEPT, &accept)
              .body(hyper::Body::empty())
          }
          None => {
            BAD_WEB_REQUEST.inc();
            http::Response::builder().status(http::StatusCode::BAD_REQUEST).body("Websocket key is not in header".into())
          }
        }
      }
      None => {
        BAD_WEB_REQUEST.inc();
        http::Response::builder().status(http::StatusCode::UNAUTHORIZED).body("Invalid or missing token. Please authenticate first".into())
      }
    }
  }
  fn parse_player_acl(acl: Vec<u8>, default: AccessControlSetting) -> std::sync::Arc<tokio::sync::Mutex<AccessControlSetting>> {
    std::sync::Arc::new(tokio::sync::Mutex::new(match rmp_serde::from_read(std::io::Cursor::new(acl.as_slice())) {
      Ok(v) => v,
      Err(e) => {
        eprintln!("ACL in database is corrupt: {}", e);
        default
      }
    }))
  }

  async fn perform_on_remote_server<R: 'static + Send, F>(&self, remote_name: String, func: F) -> R
  where
    for<'b> F: 'static + Send + FnOnce(&'b RemoteKey, &'b RemoteState) -> std::pin::Pin<Box<dyn 'b + Future<Output = R> + Send>>,
  {
    let mut states = self.remote_states.write().await;
    match self.remotes.write().await.entry(remote_name.clone()) {
      std::collections::hash_map::Entry::Occupied(mut o) => {
        let remote_id = o.get();
        match states.get(remote_id.clone()) {
          None => {
            let state = RemoteState {
              connection: tokio::sync::Mutex::new(RemoteConnection::Dead(chrono::Utc::now(), Vec::new())),
              interested_in_list: tokio::sync::Mutex::new(std::collections::HashSet::new()),
              name: remote_name,
            };
            let result = func(remote_id, &state).await;
            let id = states.insert(state);
            o.insert(id);
            result
          }
          Some(state) => func(remote_id, state).await,
        }
      }
      std::collections::hash_map::Entry::Vacant(e) => {
        let state = RemoteState {
          connection: tokio::sync::Mutex::new(RemoteConnection::Dead(chrono::Utc::now(), Vec::new())),
          interested_in_list: tokio::sync::Mutex::new(std::collections::HashSet::new()),
          name: remote_name,
        };
        let id = states.insert(state);
        let result = func(&id, &states[id.clone()]).await;
        e.insert(id);
        result
      }
    }
  }

  fn perform_player_actions(
    player: &PlayerKey,
    player_state: &mut crate::player_state::MutablePlayerState,
    realm_key: &RealmKey,
    puzzle_state: &mut crate::realm::RealmPuzzleState,
    mut actions: Vec<puzzleverse_core::Action>,
  ) {
    player_state.goal = player_state::Goal::InRealm(realm_key.clone(), player_state::RealmGoal::Idle);
    let time = chrono::Utc::now();
    puzzle_state.committed_movements.retain(|m| *m.movement.time() > time - chrono::Duration::seconds(5));
    let mut remaining_actions = if let Some((_, remaining_actions)) = puzzle_state.active_players.get_mut(&player) {
      let mut new_list = Vec::new();
      std::mem::swap(&mut new_list, remaining_actions);
      new_list
    } else {
      Vec::new()
    };

    if let Some(last_position) = puzzle_state
      .committed_movements
      .iter()
      .filter(|m| &m.player == player)
      .max_by_key(|m| m.movement.time())
      .map(|m| m.movement.end_position())
      .flatten()
      .cloned()
    {
      // If the player has no last position, they must be in liminal space and can't move
      let mut stop = false;
      let mut current_time = time;
      let mut current_position = last_position.clone();
      let mut active_proximity: std::collections::HashSet<_> = puzzle_state.manifold.active_proximity(&last_position).collect();
      let mut cleaned_actions: Vec<_> = actions
        .drain(..)
        .skip_while(|action| &last_position != action.position())
        .scan(last_position.clone(), |previous, action| {
          let current = action.position().clone();
          let result = if previous.platform == current.platform {
            if puzzle_state.manifold.verify(&current) && abs_difference(previous.x, current.x) < 2 && abs_difference(previous.y, current.y) < 2 {
              Some(action)
            } else {
              None
            }
          } else {
            if puzzle_state.manifold.verify_join(previous, &current, true) {
              Some(action)
            } else {
              None
            }
          };
          *previous = current;
          result
        })
        .collect();
      for (index, action) in cleaned_actions.drain(..).enumerate() {
        if stop {
          remaining_actions.push(action);
        } else {
          let (movement, enter_pieces, leave_pieces) = match action {
            puzzleverse_core::Action::DirectedEmote { animation, at, direction, duration } => {
              current_position = at;
              let end_time = current_time + chrono::Duration::milliseconds(duration as i64);
              let result = puzzleverse_core::CharacterMotion::DirectedEmote {
                animation: puzzleverse_core::CharacterAnimation::Custom(animation),
                at: current_position.clone(),
                direction,
                start: current_time,
              };
              current_time = end_time;
              (result, vec![], vec![])
            }
            puzzleverse_core::Action::Emote { animation, at, duration } => {
              current_position = at;
              let end_time = current_time + chrono::Duration::milliseconds(duration as i64);
              let result = puzzleverse_core::CharacterMotion::Internal {
                from: current_position.clone(),
                to: current_position.clone(),
                start: current_time,
                end: end_time.clone(),
                animation: puzzleverse_core::CharacterAnimation::Custom(animation),
              };
              current_time = end_time;
              (result, vec![], vec![])
            }
            puzzleverse_core::Action::Interaction { target, interaction, stop_on_failure } => {
              stop = stop_on_failure;
              let (animation, duration) = puzzle_state
                .manifold
                .interaction_animation(&target)
                .unwrap_or((&puzzleverse_core::CharacterAnimation::Confused, chrono::Duration::milliseconds(500)));
              let result = puzzleverse_core::CharacterMotion::Interaction {
                start: current_time.clone(),
                end: current_time + duration,
                animation: animation.clone(),
                interaction,
                at: target,
              };
              current_time = current_time + duration;
              (result, vec![], vec![])
            }
            puzzleverse_core::Action::Move(point) => {
              let (animation, duration, gated) = puzzle_state.manifold.animation(&current_position, &point, index == 0);
              let new_proximity: std::collections::HashSet<_> = puzzle_state.manifold.active_proximity(&point).collect();
              let enter_pieces = new_proximity.difference(&active_proximity).copied().collect();
              let leave_pieces = active_proximity.difference(&new_proximity).copied().collect();
              active_proximity = new_proximity;

              stop = gated;
              let result = puzzleverse_core::CharacterMotion::Internal {
                from: current_position,
                to: point.clone(),
                start: current_time,
                end: current_time + duration,
                animation: animation.clone(),
              };
              current_position = point;
              current_time = current_time + duration;
              (result, enter_pieces, leave_pieces)
            }
          };
          puzzle_state.committed_movements.push(crate::realm::PlayerMovement { player: player.clone(), movement, enter_pieces, leave_pieces });
        }
      }
    }
    if let Some((_, player_remaining_actions)) = puzzle_state.active_players.get_mut(&player) {
      std::mem::swap(player_remaining_actions, &mut remaining_actions);
    }
    player_state.goal = crate::player_state::Goal::InRealm(realm_key.clone(), crate::player_state::RealmGoal::Idle);
  }

  async fn process_client_request(
    self: &std::sync::Arc<Server>,
    player_name: &str,
    player: &PlayerKey,
    db_id: i32,
    request: puzzleverse_core::ClientRequest,
  ) -> bool {
    match self.player_states.read().await.get(player.clone()) {
      Some(player_state) => {
        let mut mutable_player_state = player_state.mutable.lock().await;
        match request {
          puzzleverse_core::ClientRequest::AccessGet { target } => {
            let acls = match &target {
              puzzleverse_core::AccessTarget::AccessServer => &self.access_acl,
              puzzleverse_core::AccessTarget::AdminServer => &self.admin_acl,
              puzzleverse_core::AccessTarget::CheckOnline => &player_state.online_acl,
              puzzleverse_core::AccessTarget::DirectMessagesServer => &self.message_acl,
              puzzleverse_core::AccessTarget::DirectMessagesUser => &player_state.message_acl,
              puzzleverse_core::AccessTarget::NewRealmDefaultAccess => &player_state.new_realm_access_acl,
              puzzleverse_core::AccessTarget::NewRealmDefaultAdmin => &player_state.new_realm_admin_acl,
              puzzleverse_core::AccessTarget::ViewLocation => &player_state.location_acl,
            }
            .lock()
            .await;
            mutable_player_state
              .connection
              .send_local(puzzleverse_core::ClientResponse::CurrentAccess { target, acls: acls.1.clone(), default: acls.0.clone() })
              .await;
            false
          }
          puzzleverse_core::ClientRequest::AccessSet { id, target, acls, default } => {
            mutable_player_state
              .connection
              .send_local(puzzleverse_core::ClientResponse::AccessChange {
                id,
                response: if match &target {
                  puzzleverse_core::AccessTarget::DirectMessagesUser => true,
                  puzzleverse_core::AccessTarget::CheckOnline => true,
                  puzzleverse_core::AccessTarget::NewRealmDefaultAccess => true,
                  puzzleverse_core::AccessTarget::NewRealmDefaultAdmin => true,
                  puzzleverse_core::AccessTarget::ViewLocation => true,
                  _ => false,
                } || {
                  let acl = &self.admin_acl.lock().await;
                  (*acl).0.check::<&str, _, _>(acl.1.iter(), player_name, &None, &self.name)
                } {
                  let encoded = rmp_serde::to_vec(&(&default, &acls)).unwrap();
                  use crate::schema::player::dsl as player_schema;
                  use crate::schema::serveracl::dsl as serveracl_schema;
                  let update_server_acl = |category: &str| {
                    let db_connection = self.db_pool.get().unwrap();
                    diesel::insert_into(serveracl_schema::serveracl)
                      .values(&(serveracl_schema::category.eq(category), serveracl_schema::acl.eq(&encoded)))
                      .on_conflict(serveracl_schema::category)
                      .do_update()
                      .set(serveracl_schema::acl.eq(&encoded))
                      .execute(&db_connection)
                  };
                  fn update_player_acl<T: diesel::query_source::Column<Table = player_schema::player, SqlType = diesel::sql_types::Binary>>(
                    server: &Server,
                    player_name: &str,
                    encoded: &[u8],
                    column: T,
                  ) -> diesel::QueryResult<usize> {
                    let db_connection = server.db_pool.get().unwrap();
                    diesel::update(player_schema::player.filter(player_schema::name.eq(player_name))).set(column.eq(encoded)).execute(&db_connection)
                  }
                  let update_result = match &target {
                    puzzleverse_core::AccessTarget::AccessServer => update_server_acl("a"),
                    puzzleverse_core::AccessTarget::AdminServer => update_server_acl("A"),
                    puzzleverse_core::AccessTarget::DirectMessagesUser => {
                      update_player_acl(&self, &player_name, &encoded, player_schema::message_acl)
                    }
                    puzzleverse_core::AccessTarget::DirectMessagesServer => update_server_acl("m"),
                    puzzleverse_core::AccessTarget::CheckOnline => update_player_acl(&self, &player_name, &encoded, player_schema::online_acl),
                    puzzleverse_core::AccessTarget::NewRealmDefaultAccess => {
                      update_player_acl(&self, &player_name, &encoded, player_schema::new_realm_access_acl)
                    }
                    puzzleverse_core::AccessTarget::NewRealmDefaultAdmin => {
                      update_player_acl(&self, &player_name, &encoded, player_schema::new_realm_admin_acl)
                    }
                    puzzleverse_core::AccessTarget::ViewLocation => update_player_acl(&self, &player_name, &encoded, player_schema::location_acl),
                  };
                  match update_result {
                    Ok(_) => {
                      *(match &target {
                        puzzleverse_core::AccessTarget::AccessServer => &self.access_acl,
                        puzzleverse_core::AccessTarget::AdminServer => &self.admin_acl,
                        puzzleverse_core::AccessTarget::DirectMessagesUser => &player_state.message_acl,
                        puzzleverse_core::AccessTarget::DirectMessagesServer => &self.message_acl,
                        puzzleverse_core::AccessTarget::CheckOnline => &player_state.online_acl,
                        puzzleverse_core::AccessTarget::NewRealmDefaultAccess => &player_state.new_realm_access_acl,
                        puzzleverse_core::AccessTarget::NewRealmDefaultAdmin => &player_state.new_realm_admin_acl,
                        puzzleverse_core::AccessTarget::ViewLocation => &player_state.location_acl,
                      }
                      .lock()
                      .await) = (default, acls);
                      puzzleverse_core::AccessChangeResponse::Changed
                    }
                    Err(e) => {
                      eprintln!("Failed to update ACL {:?}: {}", &target, e);
                      puzzleverse_core::AccessChangeResponse::InternalError
                    }
                  }
                } else {
                  puzzleverse_core::AccessChangeResponse::Denied
                },
              })
              .await;
            false
          }
          puzzleverse_core::ClientRequest::AssetCreate { id, asset_type, name, tags, licence, data } => {
            mutable_player_state
              .connection
              .send_local(match puzzleverse_core::asset::extract_children(&asset_type, &data) {
                Some(children) => {
                  use sha3::Digest;
                  let mut principal_hash = sha3::Sha3_512::new();
                  principal_hash.update(asset_type.as_bytes());
                  principal_hash.update(&[0]);
                  principal_hash.update(player_name.as_bytes());
                  principal_hash.update(&[0]);
                  principal_hash.update(self.name.as_bytes());
                  principal_hash.update(&[0]);
                  principal_hash.update(name.as_bytes());
                  principal_hash.update(&[0]);
                  principal_hash.update(&data);
                  principal_hash.update(&[0]);
                  let license_str: &[u8] = match &licence {
                    puzzleverse_core::asset::Licence::CreativeCommons(u) => match u {
                      puzzleverse_core::asset::LicenceUses::Commercial => b"cc",
                      puzzleverse_core::asset::LicenceUses::NonCommercial => b"cc-nc",
                    },
                    puzzleverse_core::asset::Licence::CreativeCommonsNoDerivatives(u) => match u {
                      puzzleverse_core::asset::LicenceUses::Commercial => b"cc-nd",
                      puzzleverse_core::asset::LicenceUses::NonCommercial => b"cc-nd-nc",
                    },
                    puzzleverse_core::asset::Licence::CreativeCommonsShareALike(u) => match u {
                      puzzleverse_core::asset::LicenceUses::Commercial => b"cc-sa",
                      puzzleverse_core::asset::LicenceUses::NonCommercial => b"cc-sa-nc",
                    },
                    puzzleverse_core::asset::Licence::PubDom => b"public",
                  };
                  principal_hash.update(license_str);
                  principal_hash.update(&[0]);
                  for tag in &tags {
                    principal_hash.update(tag.as_bytes());
                    principal_hash.update(&[0]);
                  }
                  principal_hash.update(chrono::Utc::now().to_rfc3339().as_bytes());
                  let principal = hex::encode(principal_hash.finalize());
                  self.asset_store.push(
                    &principal,
                    &puzzleverse_core::asset::Asset {
                      asset_type,
                      author: format!("{}@{}", player_name, &self.name),
                      children,
                      data,
                      licence,
                      name,
                      tags,
                    },
                  );
                  puzzleverse_core::ClientResponse::AssetCreationSucceeded { id, hash: principal }
                }
                None => puzzleverse_core::ClientResponse::AssetCreationFailed { id, error: puzzleverse_core::AssetError::Invalid },
              })
              .await;
            false
          }
          puzzleverse_core::ClientRequest::AssetPull { id } => {
            let mut retry = true;
            while retry {
              retry = false;
              match self.asset_store.pull(&id) {
                puzzleverse_core::asset_store::LoadResult::Loaded(asset) => {
                  mutable_player_state.connection.send_local(puzzleverse_core::ClientResponse::Asset(id.clone(), asset)).await;
                }
                puzzleverse_core::asset_store::LoadResult::Unknown => {
                  retry = self.find_asset(&id, AssetPullAction::PushToPlayer(player.clone())).await;
                }
                puzzleverse_core::asset_store::LoadResult::InternalError => {
                  eprintln!("Asset {} cannot be loaded", &id);
                  mutable_player_state.connection.send_local(puzzleverse_core::ClientResponse::AssetUnavailable(id.clone())).await;
                }
                puzzleverse_core::asset_store::LoadResult::Corrupt => {
                  eprintln!("Asset {} is corrupt", &id);
                  mutable_player_state.connection.send_local(puzzleverse_core::ClientResponse::AssetUnavailable(id.clone())).await;
                }
              };
            }
            false
          }
          puzzleverse_core::ClientRequest::BookmarkAdd(bookmark_type, asset) => {
            let db_connection = self.db_pool.get().unwrap();
            use crate::schema::bookmark::dsl as bookmark_schema;
            if let Err(e) = diesel::insert_into(bookmark_schema::bookmark)
              .values(&(bookmark_schema::player.eq(db_id), bookmark_schema::asset.eq(&asset), bookmark_schema::kind.eq(id_for_type(&bookmark_type))))
              .on_conflict_do_nothing()
              .execute(&db_connection)
            {
              eprintln!("Failed to write asset to database for {}: {}", player_name, e)
            }
            false
          }
          puzzleverse_core::ClientRequest::BookmarkRemove(bookmark_type, asset) => {
            let db_connection = self.db_pool.get().unwrap();
            use crate::schema::bookmark::dsl as bookmark_schema;
            if let Err(e) = diesel::delete(bookmark_schema::bookmark.filter(
              bookmark_schema::player.eq(db_id).and(bookmark_schema::asset.eq(&asset)).and(bookmark_schema::kind.eq(id_for_type(&bookmark_type))),
            ))
            .execute(&db_connection)
            {
              eprintln!("Failed to delete asset to database for {}: {}", player_name, e)
            }
            false
          }
          puzzleverse_core::ClientRequest::BookmarksGet(bookmark_type) => {
            let result = {
              let db_connection = self.db_pool.get().unwrap();
              use crate::schema::bookmark::dsl as bookmark_schema;
              bookmark_schema::bookmark
                .select(bookmark_schema::asset)
                .filter(bookmark_schema::player.eq(db_id).and(bookmark_schema::kind.eq(id_for_type(&bookmark_type))))
                .load::<String>(&db_connection)
            };
            match result {
              Err(e) => {
                eprintln!("Failed to delete asset to database for {}: {}", player_name, e)
              }
              Ok(assets) => mutable_player_state.connection.send_local(puzzleverse_core::ClientResponse::Bookmarks(bookmark_type, assets)).await,
            }
            false
          }
          puzzleverse_core::ClientRequest::Capabilities => {
            mutable_player_state
              .connection
              .send_local(puzzleverse_core::ClientResponse::Capabilities {
                server_capabilities: CAPABILITIES.iter().map(|s| s.to_string()).collect(),
              })
              .await;
            false
          }
          puzzleverse_core::ClientRequest::DirectMessageGet { player, from, to } => {
            match crate::player_state::PlayerIdentifier::new(&player, &self.name) {
              crate::player_state::PlayerIdentifier::Local(name) => {
                let db_connection = self.db_pool.get().unwrap();
                use crate::schema::localplayerchat::dsl as chat_schema;
                use crate::schema::player::dsl as player_schema;
                let query_result = chat_schema::localplayerchat
                  .select((chat_schema::body, chat_schema::created, chat_schema::recipient.eq(db_id)))
                  .filter(
                    chat_schema::created.ge(from).and(chat_schema::created.lt(to)).and(
                      chat_schema::recipient
                        .eq(db_id)
                        .and(
                          chat_schema::sender
                            .nullable()
                            .eq(player_schema::player.select(player_schema::id).filter(player_schema::name.eq(&name)).single_value()),
                        )
                        .or(
                          chat_schema::sender.eq(db_id).and(
                            chat_schema::recipient
                              .nullable()
                              .eq(player_schema::player.select(player_schema::id).filter(player_schema::name.eq(&name)).single_value()),
                          ),
                        ),
                    ),
                  )
                  .load::<(String, chrono::DateTime<chrono::Utc>, bool)>(&db_connection);
                match query_result {
                  Ok(mut messages) => {
                    mutable_player_state
                      .connection
                      .send_local(puzzleverse_core::ClientResponse::DirectMessages {
                        player,
                        messages: messages
                          .drain(..)
                          .map(|(body, timestamp, inbound)| puzzleverse_core::DirectMessage { body, inbound, timestamp })
                          .collect(),
                      })
                      .await
                  }
                  Err(e) => eprintln!("Failed to fetch messages between {} and {}: {}", &player_name, &name, e),
                }
              }
              crate::player_state::PlayerIdentifier::Remote { player, server: remote_server } => {
                let db_connection = self.db_pool.get().unwrap();
                use crate::schema::remoteplayerchat::dsl as chat_schema;
                let query_result = chat_schema::remoteplayerchat
                  .select((chat_schema::body, chat_schema::created, chat_schema::state.eq("r")))
                  .filter(
                    chat_schema::player.eq(db_id).and(chat_schema::remote_player.eq(&player)).and(chat_schema::remote_server.eq(&remote_server)),
                  )
                  .load::<(String, chrono::DateTime<chrono::Utc>, bool)>(&db_connection);
                match query_result {
                  Ok(mut messages) => {
                    mutable_player_state
                      .connection
                      .send_local(puzzleverse_core::ClientResponse::DirectMessages {
                        player,
                        messages: messages
                          .drain(..)
                          .map(|(body, timestamp, inbound)| puzzleverse_core::DirectMessage { body, inbound, timestamp })
                          .collect(),
                      })
                      .await
                  }
                  Err(e) => eprintln!("Failed to fetch messages between {} and {}@{}: {}", &player_name, &player, &remote_server, e),
                }
              }
              crate::player_state::PlayerIdentifier::Bad => (),
            }
            false
          }
          puzzleverse_core::ClientRequest::DirectMessageSend { id, recipient, body } => {
            let timestamp = chrono::Utc::now();
            mutable_player_state
              .connection
              .send_local(puzzleverse_core::ClientResponse::DirectMessageReceipt {
                id,
                status: if player_state.debuted.load(std::sync::atomic::Ordering::Relaxed) {
                  match crate::player_state::PlayerIdentifier::new(&recipient, &self.name) {
                    crate::player_state::PlayerIdentifier::Local(name) => self.send_direct_message(player_name, &name, body).await,
                    crate::player_state::PlayerIdentifier::Remote { server: remote_name, player } => {
                      let db_connection = self.db_pool.get().unwrap();
                      let was_sent = match self.remotes.read().await.get(&remote_name) {
                        Some(remote_key) => match self.remote_states.read().await.get(remote_key.clone()) {
                          None => false,
                          Some(state) => {
                            let mut locked_state = state.connection.lock().await;
                            match &mut *locked_state {
                              RemoteConnection::Online(connection) => match connection
                                .send(RemoteMessage::DirectMessage(vec![RemoteDirectMessage {
                                  sender: player_name.to_string(),
                                  recipient: player.clone(),
                                  timestamp,
                                  body: body.clone(),
                                }]))
                                .await
                              {
                                Ok(_) => true,
                                Err(e) => {
                                  eprintln!("Failed to send direct message to {}: {}", &remote_name, e);
                                  false
                                }
                              },
                              RemoteConnection::Dead(_, _) => false,
                              RemoteConnection::Offline => false,
                            }
                          }
                        },
                        None => false,
                      };
                      if !was_sent {
                        self.attempt_remote_server_connection(&remote_name).await;
                      }
                      use crate::schema::player::dsl as player_schema;
                      use crate::schema::remoteplayerchat::dsl as remoteplayerchat_schema;
                      match diesel::insert_into(remoteplayerchat_schema::remoteplayerchat)
                        .values(&(
                          remoteplayerchat_schema::player.eq(sql_not_null_int(
                            player_schema::player.select(player_schema::id).filter(player_schema::name.eq(player_name)).single_value(),
                          )),
                          remoteplayerchat_schema::remote_player.eq(&player),
                          remoteplayerchat_schema::remote_server.eq(&remote_name),
                          remoteplayerchat_schema::body.eq(&body),
                          remoteplayerchat_schema::created.eq(&timestamp),
                          remoteplayerchat_schema::state.eq(if was_sent { "o" } else { "O" }),
                        ))
                        .execute(&db_connection)
                      {
                        Ok(_) => {
                          if was_sent {
                            puzzleverse_core::DirectMessageStatus::Delivered
                          } else {
                            puzzleverse_core::DirectMessageStatus::Queued
                          }
                        }
                        Err(e) => {
                          eprintln!("Failed to write remote direct message to {} to database: {}", &remote_name, e);
                          puzzleverse_core::DirectMessageStatus::InternalError
                        }
                      }
                    }
                    crate::player_state::PlayerIdentifier::Bad => puzzleverse_core::DirectMessageStatus::UnknownRecipient,
                  }
                } else {
                  puzzleverse_core::DirectMessageStatus::Forbidden
                },
              })
              .await;
            false
          }
          puzzleverse_core::ClientRequest::DirectMessageStats => {
            let query_result = {
              let db_connection = self.db_pool.get().unwrap();
              use crate::views::lastmessage::dsl as lastmessage_schema;
              lastmessage_schema::lastmessage
                .select((lastmessage_schema::principal, lastmessage_schema::last_time))
                .filter(lastmessage_schema::id.eq(db_id))
                .load::<(String, chrono::DateTime<chrono::Utc>)>(&db_connection)
            };
            match query_result {
              Ok(mut stats) => {
                mutable_player_state
                  .connection
                  .send_local(puzzleverse_core::ClientResponse::DirectMessageStats { stats: stats.drain(..).collect() })
                  .await
              }
              Err(e) => eprintln!("Failed to fetch messages stats for {}: {}", &player_name, e),
            };
            false
          }
          puzzleverse_core::ClientRequest::InRealm(realm_request) => {
            if self.process_realm_request(&player_name, &player, None, realm_request).await {
              mutable_player_state.goal = crate::player_state::Goal::Undecided;
              mutable_player_state.connection.send_local(puzzleverse_core::ClientResponse::InTransit).await;
            }
            false
          }
          puzzleverse_core::ClientRequest::PlayerCheck(target_player) => {
            let (state, remote_start) = match player_state::PlayerIdentifier::new(&target_player, &self.name) {
              player_state::PlayerIdentifier::Local(name) => (Some(self.check_player_state(&name, player_name, None).await), None),
              player_state::PlayerIdentifier::Remote { server: remote_server, player: remote_player } => {
                match self.remotes.read().await.get(&remote_server) {
                  None => (Some(puzzleverse_core::PlayerLocationState::ServerDown), Some(remote_server)),
                  Some(remote_id) => match self.remote_states.read().await.get(remote_id.clone()) {
                    None => (Some(puzzleverse_core::PlayerLocationState::ServerDown), Some(remote_server)),
                    Some(remote_state) => {
                      remote_state
                        .connection
                        .lock()
                        .await
                        .send(RemoteMessage::OnlineStatusRequest { requester: player_name.to_string(), target: remote_player })
                        .await;
                      (None, None)
                    }
                  },
                }
              }
              player_state::PlayerIdentifier::Bad => (Some(puzzleverse_core::PlayerLocationState::Invalid), None),
            };
            if let Some(state) = state {
              mutable_player_state.connection.send_local(puzzleverse_core::ClientResponse::PlayerState { state, player: target_player }).await;
            }
            if let Some(remote_server) = remote_start {
              self.attempt_remote_server_connection(&remote_server).await;
            }
            false
          }
          puzzleverse_core::ClientRequest::PublicKeyAdd { name, der } => {
            if let Ok(_) = openssl::pkey::PKey::public_key_from_der(&der) {
              use schema::publickey::dsl as publickey_dsl;
              let db_connection = self.db_pool.get().unwrap();
              if let Err(e) = diesel::insert_into(publickey_dsl::publickey)
                .values((publickey_dsl::player.eq(db_id), publickey_dsl::name.eq(&name), publickey_dsl::public_key.eq(&der)))
                .on_conflict_do_nothing()
                .execute(&db_connection)
              {
                eprintln!("Failed to add public key: {}", e);
              }
            }
            false
          }
          puzzleverse_core::ClientRequest::PublicKeyDelete { name } => {
            use schema::publickey::dsl as publickey_dsl;
            let db_connection = self.db_pool.get().unwrap();
            if let Err(e) = diesel::delete(publickey_dsl::publickey.filter(publickey_dsl::name.eq(&name).and(publickey_dsl::player.eq(&db_id))))
              .execute(&db_connection)
            {
              eprintln!("Failed to delete public key: {}", e);
            }
            false
          }
          puzzleverse_core::ClientRequest::PublicKeyDeleteAll => {
            use schema::publickey::dsl as publickey_dsl;
            let db_connection = self.db_pool.get().unwrap();
            if let Err(e) = diesel::delete(publickey_dsl::publickey.filter(publickey_dsl::player.eq(&db_id))).execute(&db_connection) {
              eprintln!("Failed to delete all public keys: {}", e);
            }
            false
          }
          puzzleverse_core::ClientRequest::PublicKeyList => {
            use schema::publickey::dsl as publickey_dsl;
            let db_connection = self.db_pool.get().unwrap();
            let result = publickey_dsl::publickey.select(publickey_dsl::name).filter(publickey_dsl::player.eq(&db_id)).load::<String>(&db_connection);
            match result {
              Ok(keys) => {
                mutable_player_state.connection.send_local(puzzleverse_core::ClientResponse::PublicKeys(keys)).await;
              }
              Err(e) => {
                eprintln!("Failed to get public keys: {}", e);
              }
            }

            false
          }
          puzzleverse_core::ClientRequest::Quit => true,
          puzzleverse_core::ClientRequest::RealmChange { realm } => {
            if let Err(e) = self
              .move_queue
              .lock()
              .await
              .send(if player_state.debuted.load(std::sync::atomic::Ordering::Relaxed) {
                match realm {
                  puzzleverse_core::RealmTarget::Home => RealmMove::ToHome(player.clone()),
                  puzzleverse_core::RealmTarget::LocalRealm(name) => RealmMove::ToExistingRealm { player: player.clone(), realm: name, server: None },
                  puzzleverse_core::RealmTarget::RemoteRealm { realm, server: server_name } => {
                    server_name.to_lowercase();
                    RealmMove::ToExistingRealm {
                      player: player.clone(),
                      realm,
                      server: if &server_name == &self.name { None } else { Some(server_name) },
                    }
                  }
                }
              } else {
                RealmMove::ToHome(player.clone())
              })
              .await
            {
              eprintln!("Failed to queue realm change for {}: {}", &player_name, e);
            }
            false
          }
          puzzleverse_core::ClientRequest::RealmCreate { id, name, asset, seed } => {
            let db_connection = self.db_pool.get().unwrap();
            let result = Server::create_realm(&db_connection, &asset, player_name, Some(name), seed);
            mutable_player_state
              .connection
              .send_local(puzzleverse_core::ClientResponse::RealmCreation {
                id,
                status: match result {
                  Ok(id) => puzzleverse_core::RealmCreationStatus::Created(id),
                  Err(diesel::result::Error::NotFound) => puzzleverse_core::RealmCreationStatus::Duplicate,
                  Err(e) => {
                    eprintln!("Failed to create realm {} for {}: {}", &asset, player_name, e);
                    puzzleverse_core::RealmCreationStatus::InternalError
                  }
                },
              })
              .await;
            false
          }
          puzzleverse_core::ClientRequest::RealmDelete { id, target } => {
            let db_connection = self.db_pool.get().unwrap();
            let mut realms = self.realms.lock().await;
            let should_delete = if let Some(RealmKind::Loaded(realm_id)) = realms.get(&target) {
              let mut realm_states = self.realm_states.write().await;
              let should_delete = if let Some(realm_state) = realm_states.get(realm_id.clone()) {
                if &realm_state.owner == player_name {
                  let puzzle_state = realm_state.puzzle_state.lock().await;
                  let mut links: std::collections::HashMap<_, _> =
                    puzzle_state.active_players.keys().into_iter().map(|p| (p.clone(), puzzle::RealmLink::Home)).collect();
                  self.move_players_from_realm(&realm_state.owner, links.drain()).await;
                  true
                } else {
                  false
                }
              } else {
                false
              };
              if should_delete {
                realm_states.remove(realm_id.clone());
              }
              should_delete
            } else {
              false
            };
            if should_delete {
              realms.remove(&target);
            }
            let result = db_connection.transaction::<_, diesel::result::Error, _>(|| {
              use crate::schema::player::dsl as player_schema;
              use crate::schema::realm::dsl as realm_schema;
              use crate::schema::realmchat::dsl as chat_schema;
              diesel::update(
                player_schema::player.filter(
                  player_schema::realm.eq(
                    realm_schema::realm
                      .select(realm_schema::id)
                      .filter(realm_schema::principal.eq(&target).and(realm_schema::owner.eq(db_id)))
                      .single_value(),
                  ),
                ),
              )
              .set(player_schema::realm.eq::<Option<i32>>(None))
              .execute(&db_connection)?;
              diesel::delete(
                chat_schema::realmchat.filter(
                  chat_schema::realm.nullable().eq(
                    realm_schema::realm
                      .select(realm_schema::id)
                      .filter(realm_schema::principal.eq(&target).and(realm_schema::owner.eq(db_id)))
                      .single_value(),
                  ),
                ),
              )
              .execute(&db_connection)?;
              diesel::delete(realm_schema::realm.filter(realm_schema::principal.eq(&target).and(realm_schema::owner.eq(db_id))))
                .execute(&db_connection)
            });
            mutable_player_state
              .connection
              .send_local(puzzleverse_core::ClientResponse::RealmDeletion {
                id,
                ok: match result {
                  Err(e) => {
                    eprintln!("Failed to delete realm {}: {}", &target, e);
                    false
                  }
                  Ok(c) => c > 0,
                },
              })
              .await;
            false
          }
          puzzleverse_core::ClientRequest::RealmsList(source) => {
            match source {
              puzzleverse_core::RealmSource::Personal => {
                use crate::schema::realm::dsl as realm_schema;
                mutable_player_state
                  .connection
                  .send_local(puzzleverse_core::ClientResponse::RealmsAvailable {
                    display: puzzleverse_core::RealmSource::Personal,
                    realms: self.list_realms(realm_schema::owner.eq(db_id)),
                  })
                  .await
              }
              puzzleverse_core::RealmSource::LocalServer => {
                use crate::schema::realm::dsl as realm_schema;
                mutable_player_state
                  .connection
                  .send_local(puzzleverse_core::ClientResponse::RealmsAvailable {
                    display: puzzleverse_core::RealmSource::LocalServer,
                    realms: self.list_realms(realm_schema::in_directory),
                  })
                  .await
              }
              puzzleverse_core::RealmSource::RemoteServer(remote_name) => {
                remote_name.to_ascii_lowercase();
                if &remote_name == &self.name {
                  use crate::schema::realm::dsl as realm_schema;
                  mutable_player_state
                    .connection
                    .send_local(puzzleverse_core::ClientResponse::RealmsAvailable {
                      display: puzzleverse_core::RealmSource::RemoteServer(remote_name),
                      realms: self.list_realms(realm_schema::in_directory),
                    })
                    .await
                } else {
                  let p = player.clone();
                  self
                    .perform_on_remote_server(remote_name, move |_, remote| {
                      Box::pin(async move {
                        remote.interested_in_list.lock().await.insert(p);
                        remote.connection.lock().await.send(RemoteMessage::RealmsList).await
                      })
                    })
                    .await;
                }
              }
            }
            false
          }
          puzzleverse_core::ClientRequest::Servers => {
            let mut active_remotes = Vec::new();
            let remote_states = self.remote_states.read().await;
            for (remote_name, remote_key) in self.remotes.read().await.iter() {
              if let Some(remote_state) = remote_states.get(remote_key.clone()) {
                if let RemoteConnection::Online(_) = &*remote_state.connection.lock().await {
                  active_remotes.push(remote_name.clone());
                }
              }
            }
            mutable_player_state.connection.send_local(puzzleverse_core::ClientResponse::Servers(active_remotes)).await;
            false
          }
        }
      }
      None => true,
    }
  }
  async fn process_realm_request(
    self: &std::sync::Arc<Server>,
    player_name: &str,
    player: &PlayerKey,
    server_name: Option<&str>,
    request: puzzleverse_core::RealmRequest,
  ) -> bool {
    let player_states = self.player_states.read().await;
    match player_states.get(player.clone()) {
      Some(player_state) => {
        let mut mutable_player_state = player_state.mutable.lock().await;
        let realm = match &(*mutable_player_state).goal {
          crate::player_state::Goal::InRealm(realm, _) => Some((false, realm.clone())),
          crate::player_state::Goal::WaitingAssetTransfer(realm) => Some((true, realm.clone())),
          _ => None,
        };

        let force_to_undecided = match realm {
          Some((change, realm_key)) => match (*self).realm_states.read().await.get(realm_key.clone()) {
            Some(realm_state) => {
              if change {
                realm_state
                  .puzzle_state
                  .lock()
                  .await
                  .spawn_player(
                    self,
                    &realm_state.name.read().await,
                    *realm_state.in_directory.read().await,
                    &realm_key,
                    player,
                    &mut mutable_player_state,
                  )
                  .await;
              }
              match request {
                puzzleverse_core::RealmRequest::ChangeName(name, in_directory) => {
                  if &realm_state.owner == &player_state.principal || {
                    let acl = realm_state.admin_acl.lock().await;
                    (*acl).0.check(acl.1.iter(), player_name, &server_name, &self.name)
                  } {
                    let new_name = match name {
                      Some(name_value) => {
                        let mut realm_name = realm_state.name.write().await;
                        realm_name.truncate(0);
                        realm_name.push_str(&name_value);
                        name_value
                      }
                      None => realm_state.name.read().await.clone(),
                    };
                    let new_in_directory = match in_directory {
                      Some(in_directory_value) => {
                        let mut realm_in_directory = realm_state.in_directory.write().await;
                        *realm_in_directory = in_directory_value;
                        in_directory_value
                      }
                      None => *realm_state.in_directory.read().await,
                    };
                    realm_state
                      .puzzle_state
                      .lock()
                      .await
                      .process_realm_event(
                        &self,
                        realm_state.db_id,
                        Some((&player, &mut mutable_player_state)),
                        puzzleverse_core::RealmResponse::NameChanged(new_name.clone(), new_in_directory),
                      )
                      .await;
                  }
                }
                puzzleverse_core::RealmRequest::ChangeSetting(name, setting) => {
                  if &realm_state.owner == &player_state.principal || {
                    let acl = realm_state.admin_acl.lock().await;
                    (*acl).0.check(acl.1.iter(), player_name, &server_name, &self.name)
                  } {
                    let mut puzzle_state = realm_state.puzzle_state.lock().await;
                    if let Some(value) = puzzle_state.settings.get_mut(&name) {
                      if value.type_matched_update(&setting) {
                        puzzle_state
                          .process_realm_event(
                            &self,
                            realm_state.db_id,
                            Some((&player, &mut mutable_player_state)),
                            puzzleverse_core::RealmResponse::SettingChanged(name, setting),
                          )
                          .await;
                      }
                    }
                  }
                }
                puzzleverse_core::RealmRequest::ConsensualEmoteRequest { emote, player: target_player_name } => {
                  if &target_player_name != &player_state.principal {
                    if let Some(target_player_id) = self.players.read().await.get(&target_player_name) {
                      if let Some(target_player_state) = self.player_states.read().await.get(target_player_id.clone()) {
                        let mut target_mutable_player_state = target_player_state.mutable.lock().await;
                        if target_mutable_player_state.goal == player_state::Goal::InRealm(realm_key.clone(), player_state::RealmGoal::Idle) {
                          let puzzle_state = realm_state.puzzle_state.lock().await;
                          match (
                            puzzle_state.active_players.get(player).filter(|(_, actions)| actions.is_empty()).map(|(point, _)| point),
                            puzzle_state.active_players.get(target_player_id).filter(|(_, actions)| actions.is_empty()).map(|(point, _)| point),
                          ) {
                            (Some(initiator_position), Some(recipient_position)) => {
                              if initiator_position.platform == recipient_position.platform
                                && (abs_difference(initiator_position.x, recipient_position.x) < 2
                                  || abs_difference(initiator_position.y, recipient_position.y) < 2)
                              {
                                let epoch = realm_state.consent_epoch.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                mutable_player_state.goal = player_state::Goal::InRealm(
                                  realm_key.clone(),
                                  player_state::RealmGoal::ConsensualEmote {
                                    emote: emote.clone(),
                                    initiator: player.clone(),
                                    initiator_position: initiator_position.clone(),
                                    recipient_position: recipient_position.clone(),
                                    epoch,
                                  },
                                );
                                target_mutable_player_state
                                  .connection
                                  .send(
                                    self,
                                    target_player_id,
                                    puzzleverse_core::RealmResponse::ConsensualEmoteRequest { id: epoch, emote, player: player_name.to_string() },
                                  )
                                  .await;
                              }
                            }
                            _ => (),
                          }
                        }
                      }
                    }
                  }
                }
                puzzleverse_core::RealmRequest::ConsensualEmoteResponse { id, ok } => {
                  let mut puzzle_state = realm_state.puzzle_state.lock().await;
                  let (reset, update) = if let player_state::Goal::InRealm(
                    found_realm_key,
                    player_state::RealmGoal::ConsensualEmote { emote, epoch, initiator, initiator_position, recipient_position },
                  ) = &mutable_player_state.goal
                  {
                    (
                      true,
                      if ok
                        && id == *epoch
                        && found_realm_key == &realm_key
                        && puzzle_state
                          .active_players
                          .get(initiator)
                          .map(|(position, actions)| position == initiator_position && actions.is_empty())
                          .unwrap_or(false)
                        && puzzle_state
                          .active_players
                          .get(player)
                          .map(|(position, actions)| position == recipient_position && actions.is_empty())
                          .unwrap_or(false)
                      {
                        let start = chrono::Utc::now() + chrono::Duration::milliseconds(300);
                        puzzle_state.committed_movements.extend_from_slice(&[
                          realm::PlayerMovement {
                            player: initiator.clone(),
                            movement: puzzleverse_core::CharacterMotion::ConsensualEmoteInitiator {
                              start: start.clone(),
                              animation: emote.clone(),
                              at: initiator_position.clone(),
                            },
                            leave_pieces: vec![],
                            enter_pieces: vec![],
                          },
                          realm::PlayerMovement {
                            player: initiator.clone(),
                            movement: puzzleverse_core::CharacterMotion::ConsensualEmoteRecipient {
                              start: start.clone(),
                              animation: emote.clone(),
                              at: recipient_position.clone(),
                            },
                            leave_pieces: vec![],
                            enter_pieces: vec![],
                          },
                        ]);
                        true
                      } else {
                        false
                      },
                    )
                  } else {
                    (false, false)
                  };
                  if reset {
                    mutable_player_state.goal = player_state::Goal::InRealm(realm_key.clone(), player_state::RealmGoal::Idle);
                  }
                  if update {
                    let update = puzzle_state.make_update_state(&player_states);
                    puzzle_state.process_realm_event(self, realm_state.db_id, Some((player, &mut mutable_player_state)), update).await;
                  }
                }
                puzzleverse_core::RealmRequest::FollowRequest { player: target_player_name } => {
                  if &target_player_name != &player_state.principal {
                    if let Some(target_player_id) = self.players.read().await.get(&target_player_name) {
                      if let Some(target_player_state) = self.player_states.read().await.get(target_player_id.clone()) {
                        let location_acl = target_player_state.location_acl.lock().await;
                        let mut target_mutable_player_state = target_player_state.mutable.lock().await;
                        if target_mutable_player_state.goal == player_state::Goal::InRealm(realm_key.clone(), player_state::RealmGoal::Idle) {
                          // This is messy, if the player is local, we can determine if the request is okay, but if they are remote, we need the remote endpoint to do that, so we treat is as "ask" and let the remote server intercept the request.
                          let policy = match &player_state.server {
                            Some(_) => None,
                            None => puzzleverse_core::check_acls(location_acl.1.iter(), player_name, &player_state.server, &self.name),
                          };
                          match policy {
                            Some(true) => {
                              let mut puzzle_state = realm_state.puzzle_state.lock().await;
                              let start = chrono::Utc::now() + chrono::Duration::milliseconds(300);
                              puzzle_state.committed_movements.retain(|m| m.movement.time() < &start || &m.player != player);
                              let initiator_position = match puzzle_state.active_players.get_mut(player) {
                                Some((initiator_position, movements)) => {
                                  movements.clear();
                                  Some(initiator_position.clone())
                                }
                                None => None,
                              };
                              let target_position =
                                puzzle_state.active_players.get(target_player_id).map(|(target_position, _)| target_position.clone());

                              if let Some(initiator_position) = initiator_position {
                                if let Some(target_position) = target_position {
                                  let leave_pieces = puzzle_state.manifold.active_proximity(&initiator_position).collect();
                                  let enter_pieces = puzzle_state.manifold.active_proximity(&target_position).collect();
                                  let to = puzzle_state.manifold.find_adjacent_or_same(&target_position);
                                  puzzle_state.committed_movements.extend_from_slice(&[
                                    realm::PlayerMovement {
                                      player: player.clone(),
                                      leave_pieces,
                                      movement: puzzleverse_core::CharacterMotion::Leave { from: initiator_position, start: start.clone() },
                                      enter_pieces: vec![],
                                    },
                                    realm::PlayerMovement {
                                      player: player.clone(),
                                      movement: puzzleverse_core::CharacterMotion::Enter {
                                        to,
                                        end: start.clone() + chrono::Duration::milliseconds(200),
                                      },
                                      leave_pieces: vec![],
                                      enter_pieces,
                                    },
                                  ]);
                                }
                              }
                            }
                            Some(false) => (),
                            None => {
                              let epoch = realm_state.consent_epoch.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                              mutable_player_state.goal =
                                player_state::Goal::InRealm(realm_key.clone(), player_state::RealmGoal::Follow { initiator: player.clone(), epoch });
                              target_mutable_player_state
                                .connection
                                .send(
                                  self,
                                  target_player_id,
                                  puzzleverse_core::RealmResponse::FollowRequest { id: epoch, player: player_name.to_string() },
                                )
                                .await;
                            }
                          }
                        }
                      }
                    }
                  }
                }
                puzzleverse_core::RealmRequest::FollowResponse { id, ok } => {
                  let mut puzzle_state = realm_state.puzzle_state.lock().await;
                  let (reset, update) = if let player_state::Goal::InRealm(found_realm_key, player_state::RealmGoal::Follow { epoch, initiator }) =
                    &mutable_player_state.goal
                  {
                    (
                      true,
                      if ok && id == *epoch && found_realm_key == &realm_key {
                        let start = chrono::Utc::now() + chrono::Duration::milliseconds(300);
                        puzzle_state.committed_movements.retain(|m| m.movement.time() < &start || &m.player != initiator);
                        let initiator_position = match puzzle_state.active_players.get_mut(initiator) {
                          Some((initiator_position, movements)) => {
                            movements.clear();
                            Some(initiator_position.clone())
                          }
                          None => None,
                        };
                        let target_position = puzzle_state.active_players.get(player).map(|(target_position, _)| target_position.clone());
                        if let Some(initiator_position) = initiator_position {
                          if let Some(target_position) = target_position {
                            let leave_pieces = puzzle_state.manifold.active_proximity(&initiator_position).collect();
                            let enter_pieces = puzzle_state.manifold.active_proximity(&target_position).collect();
                            let to = puzzle_state.manifold.find_adjacent_or_same(&target_position);
                            puzzle_state.committed_movements.extend_from_slice(&[
                              realm::PlayerMovement {
                                player: initiator.clone(),
                                leave_pieces,
                                movement: puzzleverse_core::CharacterMotion::Leave { from: initiator_position, start: start.clone() },
                                enter_pieces: vec![],
                              },
                              realm::PlayerMovement {
                                player: initiator.clone(),
                                movement: puzzleverse_core::CharacterMotion::Enter { to, end: start.clone() + chrono::Duration::milliseconds(200) },
                                leave_pieces: vec![],
                                enter_pieces,
                              },
                            ]);
                          }
                        }
                        true
                      } else {
                        false
                      },
                    )
                  } else {
                    (false, false)
                  };
                  if reset {
                    mutable_player_state.goal = player_state::Goal::InRealm(realm_key.clone(), player_state::RealmGoal::Idle);
                  }
                  if update {
                    let update = puzzle_state.make_update_state(&player_states);
                    puzzle_state.process_realm_event(self, realm_state.db_id, Some((player, &mut mutable_player_state)), update).await;
                  }
                }
                puzzleverse_core::RealmRequest::GetAccess { target } => {
                  let acls = match target {
                    puzzleverse_core::RealmAccessTarget::Access => &realm_state.access_acl,
                    puzzleverse_core::RealmAccessTarget::Admin => &realm_state.admin_acl,
                  }
                  .lock()
                  .await;
                  mutable_player_state
                    .connection
                    .send(self, player, puzzleverse_core::RealmResponse::AccessCurrent { target, acls: acls.1.clone(), default: acls.0.clone() })
                    .await;
                }
                puzzleverse_core::RealmRequest::GetMessages { from, to } => {
                  let db_result = {
                    let db_connection = self.db_pool.get().unwrap();
                    use crate::schema::realmchat::dsl as realmchat_schema;
                    realmchat_schema::realmchat
                      .select((realmchat_schema::principal, realmchat_schema::created, realmchat_schema::body))
                      .filter(
                        realmchat_schema::realm.eq(realm_state.db_id).and(realmchat_schema::created.ge(from)).and(realmchat_schema::created.lt(to)),
                      )
                      .load::<(String, chrono::DateTime<chrono::Utc>, String)>(&db_connection)
                  };
                  match db_result {
                    Err(e) => eprintln!("Failed to get messages: {}", e),
                    Ok(mut data) => {
                      mutable_player_state
                        .connection
                        .send(
                          self,
                          player,
                          puzzleverse_core::RealmResponse::Messages(
                            data.drain(..).map(|(sender, timestamp, body)| puzzleverse_core::RealmMessage { body, sender, timestamp }).collect(),
                          ),
                        )
                        .await
                    }
                  }
                }
                puzzleverse_core::RealmRequest::Kick(kick_player) => {
                  if &realm_state.owner == &player_state.principal || {
                    let acl = realm_state.admin_acl.lock().await;
                    (*acl).0.check(acl.1.iter(), player_name, &server_name, &self.name)
                  } {
                    if let Some(kicked_player_id) = self.players.read().await.get(&kick_player) {
                      let mut links = std::collections::hash_map::HashMap::new();
                      links.insert(kicked_player_id.clone(), crate::puzzle::RealmLink::Home);
                      realm_state.puzzle_state.lock().await.yank(kicked_player_id, &mut links);
                      self.move_players_from_realm(&realm_state.owner, links.drain()).await;
                    }
                  }
                }
                puzzleverse_core::RealmRequest::NoOperation => (),
                puzzleverse_core::RealmRequest::Perform(actions) => {
                  let mut puzzle_state = realm_state.puzzle_state.lock().await;
                  Server::perform_player_actions(&player, &mut *mutable_player_state, &realm_key, &mut *puzzle_state, actions)
                }
                puzzleverse_core::RealmRequest::SendMessage(body) => {
                  realm_state
                    .puzzle_state
                    .lock()
                    .await
                    .process_realm_event(
                      &self,
                      realm_state.db_id,
                      Some((&player, &mut mutable_player_state)),
                      puzzleverse_core::RealmResponse::MessagePosted {
                        sender: match server_name {
                          Some(server) => {
                            format!("{}@{}", player_name, server)
                          }
                          None => player_name.to_string(),
                        },
                        body,
                        timestamp: chrono::Utc::now(),
                      },
                    )
                    .await
                }
                puzzleverse_core::RealmRequest::SetAccess { id, target, acls, default } => {
                  mutable_player_state
                    .connection
                    .send(
                      self,
                      player,
                      puzzleverse_core::RealmResponse::AccessChange {
                        id,
                        response: if &realm_state.owner == &player_state.principal || {
                          let acl = realm_state.admin_acl.lock().await;
                          (*acl).0.check(acl.1.iter(), player_name, &server_name, &self.name)
                        } {
                          use crate::schema::realm::dsl as realm_schema;
                          fn update_realm_acl<T: diesel::Column<Table = realm_schema::realm, SqlType = diesel::sql_types::Binary>>(
                            server: &Server,
                            id: i32,
                            default: puzzleverse_core::AccessDefault,
                            acls: Vec<puzzleverse_core::AccessControl>,
                            column: T,
                          ) -> puzzleverse_core::AccessChangeResponse {
                            let db_connection = server.db_pool.get().unwrap();
                            let output = rmp_serde::to_vec::<crate::AccessControlSetting>(&(default, acls)).unwrap();
                            match diesel::update(realm_schema::realm.filter(realm_schema::id.eq(id))).set(column.eq(&output)).execute(&db_connection)
                            {
                              Err(e) => {
                                println!("Failed to update realm name: {}", e);
                                puzzleverse_core::AccessChangeResponse::Changed
                              }
                              Ok(_) => puzzleverse_core::AccessChangeResponse::InternalError,
                            }
                          }
                          match target {
                            puzzleverse_core::RealmAccessTarget::Access => {
                              update_realm_acl(&self, realm_state.db_id, default, acls, realm_schema::access_acl)
                            }
                            puzzleverse_core::RealmAccessTarget::Admin => {
                              update_realm_acl(&self, realm_state.db_id, default, acls, realm_schema::admin_acl)
                            }
                          }
                        } else {
                          puzzleverse_core::AccessChangeResponse::Denied
                        },
                      },
                    )
                    .await
                }
              }
              false
            }
            None => true,
          },
          None => false,
        };
        if force_to_undecided {
          // Somehow, the player is in a dead realm; kick them into transit mode
          mutable_player_state.goal = crate::player_state::Goal::Undecided;
          mutable_player_state.connection.release_player(&self).await;
        }
        false
      }
      None => true,
    }
  }
  async fn process_server_message(self: &std::sync::Arc<Server>, server_name: &str, remote_id: &crate::RemoteKey, req: RemoteMessage) {
    async fn resolve_remote_player(server: &std::sync::Arc<Server>, remote_id: &RemoteKey, player: &str, server_name: &str) -> PlayerKey {
      match server.players.write().await.entry(format!("{}@{}", &player, server_name)) {
        std::collections::hash_map::Entry::Occupied(o) => o.get().clone(),
        std::collections::hash_map::Entry::Vacant(v) => {
          let player_key = server.player_states.write().await.insert(crate::player_state::PlayerState {
            principal: format!("{}@{}", &player, server_name),
            name: player.to_string(),
            debuted: true.into(),
            server: Some(server_name.into()),
            mutable: tokio::sync::Mutex::new(crate::player_state::MutablePlayerState {
              goal: crate::player_state::Goal::Undecided,
              connection: crate::player_state::PlayerConnection::Remote(player.to_string(), remote_id.clone()),
            }),
            message_acl: std::sync::Arc::new(tokio::sync::Mutex::new((puzzleverse_core::AccessDefault::Allow, vec![]))),
            online_acl: std::sync::Arc::new(tokio::sync::Mutex::new((puzzleverse_core::AccessDefault::Allow, vec![]))),
            location_acl: std::sync::Arc::new(tokio::sync::Mutex::new((puzzleverse_core::AccessDefault::Allow, vec![]))),
            new_realm_access_acl: std::sync::Arc::new(tokio::sync::Mutex::new((puzzleverse_core::AccessDefault::Allow, vec![]))),
            new_realm_admin_acl: std::sync::Arc::new(tokio::sync::Mutex::new((puzzleverse_core::AccessDefault::Allow, vec![]))),
          });
          v.insert(player_key.clone());
          player_key
        }
      }
    }
    match req {
      RemoteMessage::AssetsPull { mut assets } => {
        let mut output = std::collections::HashMap::new();
        for asset in assets.drain(..) {
          if let puzzleverse_core::asset_store::LoadResult::Loaded(value) = self.asset_store.pull(&asset) {
            output.insert(asset, value);
          }
        }
        if output.len() > 0 {
          if let Some(remote_state) = self.remote_states.read().await.get(remote_id.clone()) {
            remote_state.connection.lock().await.send(RemoteMessage::AssetsPush { assets: output }).await;
          }
        }
      }
      RemoteMessage::AssetsPush { mut assets } => {
        let mut post_insert_actions = Vec::new();
        {
          let mut outstanding_assets = self.outstanding_assets.lock().await;
          for (asset, value) in assets.drain() {
            if !self.asset_store.check(&asset) {
              self.asset_store.push(&asset, &value);
            }
            if let Some(mut actions) = outstanding_assets.remove(&asset) {
              post_insert_actions.extend(actions.drain(..).map(|action| (asset.clone(), value.clone(), action)));
            }
          }
        }
        for (asset_name, asset_value, action) in post_insert_actions.drain(..) {
          match action {
            AssetPullAction::PushToPlayer(player) => {
              if let Some(player_state) = self.player_states.read().await.get(player) {
                player_state.mutable.lock().await.connection.send_local(puzzleverse_core::ClientResponse::Asset(asset_name, asset_value)).await;
              }
            }
            AssetPullAction::LoadRealm(realm, counter) => {
              let missing_children: Vec<_> = asset_value.children.iter().filter(|ca| !self.asset_store.check(ca)).collect();
              counter.fetch_add(missing_children.len(), std::sync::atomic::Ordering::Relaxed);
              for missing_child in missing_children {
                if self.find_asset(missing_child, AssetPullAction::LoadRealm(realm.clone(), counter.clone())).await {
                  if counter.fetch_sub(1, std::sync::atomic::Ordering::Relaxed) <= 1 {
                    panic!("Invalid reference count for realm asset tracker");
                  }
                }
              }
              if counter.fetch_sub(1, std::sync::atomic::Ordering::Relaxed) <= 1 {
                // We were the last asset, trigger this to run
                self.clone().load_realm(realm).await;
              }
            }
          }
        }
      }
      RemoteMessage::DirectMessage(mut messages) => {
        let result = {
          let db_connection = self.db_pool.get().unwrap();
          use crate::schema::player::dsl as player_schema;
          player_schema::player
            .select((player_schema::name, player_schema::id))
            .filter(player_schema::name.eq_any(messages.iter().map(|m| &m.recipient).collect::<Vec<_>>()))
            .load::<(String, i32)>(&db_connection)
        };
        match result {
          Err(e) => eprintln!("Failed to get users to write messages from {}: {}", &server_name, e),
          Ok(mut user_info) => {
            let ids_for_user: std::collections::HashMap<_, _> = user_info.drain(..).collect();
            let result = {
              let db_connection = self.db_pool.get().unwrap();
              use crate::schema::player::dsl as player_schema;
              use crate::schema::remoteplayerchat::dsl as chat_schema;
              diesel::insert_into(chat_schema::remoteplayerchat)
                .values(
                  messages
                    .drain(..)
                    .flat_map(|m| {
                      ids_for_user.get(&m.recipient).map(|id| {
                        (
                          chat_schema::player.eq(id),
                          chat_schema::remote_server.eq(&server_name),
                          chat_schema::remote_player.eq(m.sender),
                          chat_schema::body.eq(m.body),
                          chat_schema::created.eq(m.timestamp),
                          chat_schema::state.eq("r"),
                        )
                      })
                    })
                    .collect::<Vec<_>>(),
                )
                .on_conflict_do_nothing()
                .returning((
                  sql_not_null_str(
                    player_schema::player.select(player_schema::name).filter(player_schema::id.eq(chat_schema::player)).single_value(),
                  ),
                  chat_schema::remote_player,
                  chat_schema::body,
                  chat_schema::created,
                ))
                .load::<(String, String, String, chrono::DateTime<chrono::Utc>)>(&db_connection)
            };
            match result {
              Err(e) => {
                eprintln!("Failed to write messages from {}: {}", &server_name, e)
              }
              Ok(mut written) => {
                let mut output = std::collections::HashMap::new();
                for (mut sender, recipient, body, timestamp) in written.drain(..) {
                  sender.push('@');
                  sender.push_str(&server_name);
                  (match (match output.entry(recipient) {
                    std::collections::hash_map::Entry::Vacant(e) => e.insert(std::collections::HashMap::new()),
                    std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
                  })
                  .entry(sender)
                  {
                    std::collections::hash_map::Entry::Vacant(e) => e.insert(Vec::new()),
                    std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
                  })
                  .push(puzzleverse_core::DirectMessage { inbound: true, body, timestamp });
                }
                for (recipient, mut data) in output.drain() {
                  if let Some(player_id) = self.players.read().await.get(&recipient) {
                    if let Some(player_state) = self.player_states.read().await.get(player_id.clone()) {
                      let mut mutable_player_state = player_state.mutable.lock().await;
                      for (sender, messages) in data.drain() {
                        mutable_player_state
                          .connection
                          .send_local(puzzleverse_core::ClientResponse::DirectMessages { player: sender, messages })
                          .await;
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
      RemoteMessage::OnlineStatusRequest { requester, target } => {
        let state = self.check_player_state(&target, &requester, Some(server_name)).await;
        if let Some(remote_state) = self.remote_states.read().await.get(remote_id.clone()) {
          remote_state.connection.lock().await.send(RemoteMessage::OnlineStatusResponse { requester, target, state }).await;
        }
      }
      RemoteMessage::OnlineStatusResponse { requester, mut target, state } => {
        if let Some(player_id) = self.players.read().await.get(&requester) {
          if let Some(player_state) = self.player_states.read().await.get(player_id.clone()) {
            target.push('@');
            target.push_str(&server_name);
            player_state.mutable.lock().await.connection.send_local(puzzleverse_core::ClientResponse::PlayerState { player: target, state }).await;
          }
        }
      }
      RemoteMessage::RealmChanged { player, change } => {
        self
          .send_response_to_player_visiting_remote(
            remote_id,
            &server_name,
            &player,
            match &change {
              puzzleverse_core::RealmChange::Success { .. } => None,
              puzzleverse_core::RealmChange::Denied => Some(player_state::Goal::Undecided),
            },
            puzzleverse_core::ClientResponse::RealmChanged(change),
          )
          .await;
      }
      RemoteMessage::RealmRequest { player, request } => {
        let player_id = resolve_remote_player(&self, &remote_id, &player, &server_name).await;
        if self.process_realm_request(&player, &player_id, Some(&server_name), request).await {
          let player_states = self.player_states.read().await;
          let mut mutable_player_state = player_states.get(player_id).unwrap().mutable.lock().await;
          mutable_player_state.goal = player_state::Goal::Undecided;
          mutable_player_state.connection.release_player(self).await;
        }
      }
      RemoteMessage::RealmResponse { player, response } => {
        self
          .send_response_to_player_visiting_remote(remote_id, &server_name, &player, None, puzzleverse_core::ClientResponse::InRealm(response))
          .await
      }
      RemoteMessage::RealmsAvailable(realms) => {
        if let Some(remote_state) = self.remote_states.read().await.get(remote_id.clone()) {
          for player in remote_state.interested_in_list.lock().await.drain() {
            if let Some(player_state) = self.player_states.read().await.get(player) {
              player_state
                .mutable
                .lock()
                .await
                .connection
                .send_local(puzzleverse_core::ClientResponse::RealmsAvailable {
                  display: puzzleverse_core::RealmSource::RemoteServer(server_name.to_string()),
                  realms: realms.clone(),
                })
                .await;
            }
          }
        }
      }
      RemoteMessage::RealmsList => {
        if let Some(remote_state) = self.remote_states.read().await.get(remote_id.clone()) {
          use crate::schema::realm::dsl as realm_schema;
          remote_state.connection.lock().await.send(RemoteMessage::RealmsAvailable(self.list_realms(realm_schema::in_directory))).await;
        }
      }
      RemoteMessage::VisitorCheckAssets { player, assets } => {
        let missing_assets: Vec<_> = assets.iter().filter(|a| !self.asset_store.check(a)).cloned().collect();
        if missing_assets.len() > 0 {
          if let Some(remote_state) = self.remote_states.read().await.get(remote_id.clone()) {
            remote_state.connection.lock().await.send(RemoteMessage::AssetsPull { assets: missing_assets }).await;
          }
        }
        self
          .send_response_to_player_visiting_remote(
            remote_id,
            &server_name,
            &player,
            None,
            puzzleverse_core::ClientResponse::CheckAssets { asset: assets },
          )
          .await
      }
      RemoteMessage::VisitorRelease(player, target) => {
        if let Some(player_id) = self.players.read().await.get(&player) {
          if let Some(playerstate) = self.player_states.read().await.get(*player_id) {
            let mut playerstate_mutable = playerstate.mutable.lock().await;
            match playerstate_mutable.goal {
              crate::player_state::Goal::OnRemote(remote_key, _) => {
                if remote_id == &remote_key {
                  let mut set_dead = None;
                  if let player_state::PlayerConnection::Local(db_id, connection, _) = &mut playerstate_mutable.connection {
                    if let Err(e) = connection.send(puzzleverse_core::ClientResponse::InTransit).await {
                      eprintln!("Failed to send to player {}: {}", &playerstate.principal, e);
                      set_dead = Some(*db_id);
                    }
                    let (new_goal, change) = self.resolve_realm(&player_id, &player, Some(server_name.to_string()), *db_id, target).await;
                    playerstate_mutable.goal = new_goal;
                    if let Some(change) = change {
                      playerstate_mutable.connection.send_change(self, change).await;
                    }
                  }
                  if let Some(db_id) = set_dead {
                    playerstate_mutable.connection =
                      player_state::PlayerConnection::LocalDead(db_id, chrono::Utc::now(), vec![puzzleverse_core::ClientResponse::InTransit]);
                  }
                }
              }
              _ => (),
            }
          }
        }
      }
      RemoteMessage::VisitorSend { player, realm } => {
        let allowed = {
          let access_acl = self.access_acl.lock().await;
          access_acl.0.check(access_acl.1.iter(), &player, &Some(server_name), &self.name)
        };
        if allowed {
          let player_id = resolve_remote_player(&self, &remote_id, &player, &server_name).await;
          if let Err(e) = self.move_queue.lock().await.send(RealmMove::ToExistingRealm { player: player_id, realm, server: None }).await {
            eprintln!("Failed to move player {} to new realm from server {}: {}", &player, &server_name, e);
          }
        } else {
          if let Some(remote_state) = self.remote_states.read().await.get(remote_id.clone()) {
            remote_state.connection.lock().await.send(RemoteMessage::VisitorRelease(player, ReleaseTarget::Transit)).await;
          }
        }
      }
      RemoteMessage::VisitorYank(player) => {
        if let Some(player_id) = self.players.read().await.get(&format!("{}@{}", &player, server_name)) {
          if let Some(mut playerstate) = match self.player_states.read().await.get(*player_id) {
            Some(m) => Some(m.mutable.lock().await),
            None => None,
          } {
            match playerstate.goal {
              crate::player_state::Goal::InRealm(realm, _) => {
                if let Some(realm_state) = self.realm_states.read().await.get(realm) {
                  let mut puzzle_state = realm_state.puzzle_state.lock().await;
                  let mut links = std::collections::hash_map::HashMap::new();
                  links.insert(player_id.clone(), crate::puzzle::RealmLink::Home);
                  puzzle_state.yank(player_id, &mut links);
                  self.move_players_from_realm(&realm_state.owner, links.drain()).await;
                }
              }
              _ => (),
            }
            playerstate.goal = crate::player_state::Goal::OnRemote(remote_id.clone(), None);
          }
        }
      }
    }
  }
  async fn resolve_realm(
    self: &std::sync::Arc<Server>,
    player_id: &PlayerKey,
    player_name: &str,
    server_name: Option<String>,
    db_id: i32,
    target: ReleaseTarget,
  ) -> (player_state::Goal, Option<puzzleverse_core::RealmChange>) {
    match target {
      ReleaseTarget::Home => {
        use crate::schema::player::dsl as player_schema;
        use crate::schema::realm::dsl as realm_schema;
        self
          .clone()
          .find_or_create_realm(
            player_id.clone(),
            player_name,
            server_name,
            Some((DEFAULT_HOME, player_name.to_string())),
            realm_schema::id.nullable().eq(player_schema::player.select(player_schema::realm).filter(player_schema::id.eq(db_id)).single_value()),
          )
          .await
      }
      ReleaseTarget::Transit => (player_state::Goal::Undecided, Some(puzzleverse_core::RealmChange::Denied)),
      ReleaseTarget::Realm(realm, server_name) => {
        if &server_name == &self.name {
          use crate::schema::realm::dsl as realm_schema;
          self.clone().find_or_create_realm(player_id.clone(), player_name, Some(server_name), None, realm_schema::principal.eq(realm)).await
        } else {
          // Request hand-off to server
          let player = player_name.to_string();
          self
            .perform_on_remote_server(server_name, move |remote_id, remote| {
              Box::pin(async move {
                remote.connection.lock().await.send(RemoteMessage::VisitorSend { player, realm: realm.clone() }).await;
                (player_state::Goal::OnRemote(remote_id.clone(), Some(realm)), None)
              })
            })
            .await
        }
      }
    }
  }

  async fn send_direct_message(&self, sender: &str, recipient: &str, body: String) -> puzzleverse_core::DirectMessageStatus {
    use crate::schema::localplayerchat::dsl as chat_schema;
    use crate::schema::player::dsl as player_schema;
    if sender == recipient {
      return puzzleverse_core::DirectMessageStatus::UnknownRecipient;
    }
    let db_result = {
      let db_connection = self.db_pool.get().unwrap();
      player_schema::player.select(player_schema::id).filter(player_schema::name.eq(recipient)).first::<i32>(&db_connection)
    };
    match db_result {
      Err(diesel::NotFound) => puzzleverse_core::DirectMessageStatus::UnknownRecipient,
      Err(e) => {
        eprintln!("Failed to query player {}: {}", recipient, e);
        puzzleverse_core::DirectMessageStatus::InternalError
      }
      Ok(recipient_id) => {
        let timestamp = chrono::Utc::now();
        let insert_result = {
          let db_connection = self.db_pool.get().unwrap();
          diesel::insert_into(chat_schema::localplayerchat)
            .values(&(
              chat_schema::recipient.eq(recipient_id),
              chat_schema::body.eq(&body),
              chat_schema::created.eq(&timestamp),
              chat_schema::sender
                .eq(sql_not_null_int(player_schema::player.select(player_schema::id).filter(player_schema::name.eq(sender)).single_value())),
            ))
            .execute(&db_connection)
        };
        match insert_result {
          Err(e) => {
            eprintln!("Failed to insert message from {} to {}: {}", sender, recipient, e);
            puzzleverse_core::DirectMessageStatus::InternalError
          }
          Ok(_) => {
            match self.players.read().await.get(recipient) {
              None => {}
              Some(recipient_key) => match self.player_states.read().await.get(recipient_key.clone()) {
                None => {}
                Some(state) => {
                  state
                    .mutable
                    .lock()
                    .await
                    .connection
                    .send_local(puzzleverse_core::ClientResponse::DirectMessageReceived { sender: sender.to_string(), body, timestamp })
                    .await
                }
              },
            }
            puzzleverse_core::DirectMessageStatus::Delivered
          }
        }
      }
    }
  }

  async fn send_response_to_player_visiting_remote(
    &self,
    remote_id: &RemoteKey,
    remote_name: &str,
    player: &str,
    goal: Option<player_state::Goal>,
    response: puzzleverse_core::ClientResponse,
  ) {
    if match self.players.read().await.get(player) {
      Some(player_id) => match self.player_states.read().await.get(player_id.clone()) {
        None => true,
        Some(playerstate) => {
          let mut state = playerstate.mutable.lock().await;
          match state.goal {
            crate::player_state::Goal::OnRemote(selected_remote, _) => {
              if &selected_remote == remote_id {
                let forward =
                  if let puzzleverse_core::ClientResponse::InRealm(puzzleverse_core::RealmResponse::FollowRequest { id, player }) = &response {
                    if let Some((name, server_name)) = match crate::player_state::PlayerIdentifier::new(&player, remote_name) {
                      player_state::PlayerIdentifier::Local(name) => Some((name, remote_name.to_string())),
                      player_state::PlayerIdentifier::Remote { server, player } => Some((player, server)),
                      player_state::PlayerIdentifier::Bad => None,
                    } {
                      let location_acl = playerstate.location_acl.lock().await;
                      match puzzleverse_core::check_acls((*location_acl).1.iter(), &name, &Some(server_name), &self.name) {
                        Some(ok) => {
                          if let Some(state) = self.remote_states.read().await.get(remote_id.clone()) {
                            state
                              .connection
                              .lock()
                              .await
                              .send(RemoteMessage::RealmRequest {
                                player: player.clone(),
                                request: puzzleverse_core::RealmRequest::FollowResponse { id: *id, ok },
                              })
                              .await;
                          }
                          false
                        }
                        None => true,
                      }
                    } else {
                      false
                    }
                  } else {
                    true
                  };
                if forward {
                  state.connection.send_local(response).await;
                  if let Some(goal) = goal {
                    state.goal = goal;
                  }
                }
                false
              } else {
                true
              }
            }
            _ => true,
          }
        }
      },
      None => true,
    } {
      // If the remote server thinks a player is active and we don't, then tell that server we want the player back
      if let Some(state) = self.remote_states.read().await.get(remote_id.clone()) {
        state.connection.lock().await.send(RemoteMessage::VisitorYank(player.to_string())).await;
      }
    }
  }
  fn signing_key(&self, delta: u8) -> String {
    format!("{}{}{}", &self.name, (chrono::Utc::now() - self.startup).num_minutes() / 2 - delta as i64, &self.startup)
  }
}
/// Start the server. This is in a separate function from main because the tokio annotation mangles compile error information
async fn start() -> Result<(), Box<dyn std::error::Error>> {
  BUILD_ID.with_label_values(&[&build_id::get().to_string()]).inc();
  let configuration: ServerConfiguration = {
    let mut configuration_file: String = "puzzleverse.config".into();
    {
      let mut ap = argparse::ArgumentParser::new();
      ap.set_description("Puzzleverse Server");
      ap.refer(&mut configuration_file).add_option(&["-c", "--config"], argparse::Store, "Set the configuration JSON file");
      ap.parse_args_or_exit();
    }
    serde_json::from_reader(std::fs::File::open(&configuration_file).expect("Cannot open configuration file"))
      .expect("Cannot parse configuration file.")
  };

  let db_pool = {
    let manager = diesel::r2d2::ConnectionManager::<diesel::pg::PgConnection>::new(configuration.database_url);
    r2d2::Pool::builder().build(manager).expect("Failed to create pool.")
  };

  fn read_acl(
    db_pool: &r2d2::Pool<diesel::r2d2::ConnectionManager<diesel::pg::PgConnection>>,
    category: &str,
    default: AccessControlSetting,
  ) -> std::sync::Arc<tokio::sync::Mutex<AccessControlSetting>> {
    let connection = db_pool.get().unwrap();
    use crate::schema::serveracl::dsl as serveracl_schema;

    std::sync::Arc::new(tokio::sync::Mutex::new(
      match serveracl_schema::serveracl.select(serveracl_schema::acl).filter(serveracl_schema::category.eq(category)).first::<Vec<u8>>(&connection) {
        Err(e) => {
          eprintln!("Failed to get server ACL: {}", e);
          default
        }
        Ok(acl) => match rmp_serde::from_read(std::io::Cursor::new(acl.as_slice())) {
          Ok(v) => v,
          Err(e) => {
            eprintln!("ACL in database is corrupt: {}", e);
            default
          }
        },
      },
    ))
  }

  configuration.name.to_lowercase();

  let (spawn_sender, spawn_receiver) = tokio::sync::mpsc::channel(100);
  let (asset_sender, asset_receiver) = tokio::sync::mpsc::channel(100);
  let jwt_secret = rand::thread_rng().gen::<[u8; 32]>();
  let server = std::sync::Arc::new(Server {
    asset_store: Box::new(puzzleverse_core::asset_store::FileSystemStore::new(&std::path::Path::new(&configuration.asset_store), &[4, 4, 8])),
    outstanding_assets: tokio::sync::Mutex::new(std::collections::HashMap::new()),
    push_assets: tokio::sync::Mutex::new(asset_sender),
    authentication: configuration.authentication.load().unwrap(),
    jwt_decoding_key: jsonwebtoken::DecodingKey::from_secret(&jwt_secret).into_static(),
    jwt_encoding_key: jsonwebtoken::EncodingKey::from_secret(&jwt_secret),
    name: configuration.name,
    players: tokio::sync::RwLock::new(std::collections::HashMap::new()),
    player_states: tokio::sync::RwLock::new(slotmap::DenseSlotMap::with_key()),
    move_queue: tokio::sync::Mutex::new(spawn_sender),
    realms: tokio::sync::Mutex::new(std::collections::HashMap::new()),
    realm_states: tokio::sync::RwLock::new(slotmap::DenseSlotMap::with_key()),
    remotes: tokio::sync::RwLock::new(std::collections::HashMap::new()),
    remote_states: tokio::sync::RwLock::new(slotmap::DenseSlotMap::with_key()),
    attempting_remote_contacts: tokio::sync::Mutex::new(std::collections::HashSet::new()),
    admin_acl: read_acl(&db_pool, "A", (puzzleverse_core::AccessDefault::Deny, vec![puzzleverse_core::AccessControl::AllowLocal(None)])),
    access_acl: read_acl(&db_pool, "a", (puzzleverse_core::AccessDefault::Allow, vec![])),
    message_acl: read_acl(&db_pool, "m", (puzzleverse_core::AccessDefault::Allow, vec![])),
    db_pool,
    move_epoch: std::sync::atomic::AtomicU64::new(0),
    startup: chrono::Utc::now(),
  });
  embedded_migrations::run(&*server.db_pool.clone().get().unwrap()).unwrap();
  start_player_mover_task(&server, spawn_receiver);
  start_asset_puller_task(&server, asset_receiver);
  start_message_cleaner_task(&server);
  let addr =
    configuration.bind_address.unwrap_or(if configuration.certificate.is_none() { "0.0.0.0:80".to_string() } else { "0.0.0.0:443".to_string() });

  // Create two different forms of the server depending on whether we are using HTTP or HTTPS
  let webserver: std::pin::Pin<Box<dyn Future<Output = hyper::Result<()>>>> = match configuration.certificate {
    Some(certificate_path) => {
      let acceptor = std::sync::Arc::new(std::sync::RwLock::new(load_cert(&certificate_path)?));
      let server = server.clone();
      let (tx, rx) = std::sync::mpsc::channel();
      match notify::watcher(tx, std::time::Duration::from_secs(30)) {
        Ok(mut watcher) => {
          use notify::Watcher;
          watcher.watch(&certificate_path, notify::RecursiveMode::NonRecursive).unwrap();
          let a = acceptor.clone();
          std::thread::spawn(move || loop {
            match rx.recv() {
              Ok(event) => {
                if let Some(file) = match event {
                  notify::DebouncedEvent::Create(p) => Some(std::borrow::Cow::from(p)),
                  notify::DebouncedEvent::Write(p) => Some(std::borrow::Cow::from(p)),
                  notify::DebouncedEvent::Rescan => Some(std::borrow::Cow::from(&certificate_path)),
                  _ => None,
                } {
                  match load_cert(file) {
                    Ok(acceptor) => {
                      *a.write().unwrap() = acceptor;
                    }
                    Err(e) => eprintln!("Failed to load new SSL cert: {}", e),
                  }
                }
              }
              Err(e) => {
                eprintln!("SSL certificate loader died: {}", e);
                break;
              }
            }
          });
        }
        Err(e) => eprintln!("Failed to set up watcher on SSL cert: {}", e),
      }

      struct TokioTcpListener(tokio::net::TcpListener);
      impl futures::Stream for TokioTcpListener {
        type Item = tokio::net::TcpStream;

        fn poll_next(self: std::pin::Pin<&mut Self>, cx: &mut futures::task::Context<'_>) -> futures::task::Poll<Option<Self::Item>> {
          self.0.poll_accept(cx).map(|result| match result {
            Ok((socket, _)) => Some(socket),
            Err(e) => {
              eprintln!("Failed to accept TCP request: {}", e);
              None
            }
          })
        }
      }

      Box::pin(
        hyper::server::Server::builder(hyper::server::accept::from_stream(TokioTcpListener(tokio::net::TcpListener::bind(&addr).await?).then(
          move |socket| {
            let acceptor = acceptor.clone();
            async move { acceptor.read().unwrap().accept(socket).await }
          },
        )))
        .serve(hyper::service::make_service_fn(move |_| {
          let server = server.clone();
          async move {
            Ok::<_, std::convert::Infallible>(hyper::service::service_fn(move |req: http::Request<hyper::Body>| {
              let server = server.clone();
              server.handle_http_request(req)
            }))
          }
        })),
      )
    }
    None => {
      let server = server.clone();
      let addr = addr.parse().expect("Invalid bind address");
      Box::pin(hyper::Server::bind(&addr).serve(hyper::service::make_service_fn(move |_| {
        let server = server.clone();
        async move {
          Ok::<_, std::convert::Infallible>(hyper::service::service_fn(move |req: http::Request<hyper::Body>| {
            let server = server.clone();
            server.handle_http_request(req)
          }))
        }
      })))
    }
  };

  for remote in {
    let db_connection = server.db_pool.get().unwrap();
    use crate::schema::remoteplayerchat::dsl as chat_schema;
    chat_schema::remoteplayerchat
      .select(chat_schema::remote_server)
      .distinct()
      .filter(chat_schema::state.eq("O"))
      .load::<String>(&db_connection)
      .unwrap()
  } {
    let server = server.clone();
    tokio::spawn(async move { server.attempt_remote_server_connection(&remote).await });
  }
  webserver.await.map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
}
fn start_asset_puller_task(server: &std::sync::Arc<Server>, mut asset_receiver: tokio::sync::mpsc::Receiver<(PlayerKey, String)>) {
  let s = std::sync::Arc::downgrade(server);
  tokio::spawn(async move {
    loop {
      match asset_receiver.recv().await {
        None => break,
        Some((player, asset)) => match s.upgrade() {
          None => break,
          Some(server) => {
            match server.asset_store.pull(&asset) {
              puzzleverse_core::asset_store::LoadResult::Loaded(loaded) => match server.player_states.read().await.get(player) {
                Some(player_state) => {
                  player_state
                    .mutable
                    .lock()
                    .await
                    .connection
                    .send_assets(&server, loaded.children.iter().cloned().chain(std::iter::once(asset)))
                    .await
                }
                None => eprintln!("Trying to send asset {} to non-existent player", asset),
              },
              puzzleverse_core::asset_store::LoadResult::Unknown => {
                if server.find_asset(&asset, AssetPullAction::PushToPlayer(player.clone())).await {
                  // Race condition; this was added while were busy. Cycle it through the queue again
                  if let Err(e) = server.push_assets.lock().await.send((player, asset)).await {
                    eprintln!("Failed to cycle asset request: {}", e);
                  }
                }
              }
              _ => {
                eprintln!("Age is using bad asset: {}", asset);
              }
            }
          }
        },
      }
    }
  });
}
fn start_message_cleaner_task(server: &std::sync::Arc<Server>) {
  let s = std::sync::Arc::downgrade(server);
  tokio::spawn(async move {
    let mut counter: u32 = 0;
    loop {
      tokio::time::sleep(std::time::Duration::from_secs(1)).await;
      if let Some(server) = s.upgrade() {
        counter += 1;
        if counter > 600 {
          counter = 0;
          use crate::schema::localplayerchat::dsl as local_player_chat_schema;
          use crate::schema::realmchat::dsl as realm_chat_schema;
          use crate::schema::remoteplayerchat::dsl as remote_player_chat_schema;
          let db = server.db_pool.get().unwrap();
          let horizon = chrono::Utc::now() - chrono::Duration::days(30);
          if let Err(e) = diesel::delete(realm_chat_schema::realmchat.filter(realm_chat_schema::created.le(&horizon))).execute(&db) {
            eprintln!("Failed to delete old realm chats: {}", e);
          }
          if let Err(e) =
            diesel::delete(local_player_chat_schema::localplayerchat.filter(local_player_chat_schema::created.le(&horizon))).execute(&db)
          {
            eprintln!("Failed to delete old local player chats: {}", e);
          }
          if let Err(e) =
            diesel::delete(remote_player_chat_schema::remoteplayerchat.filter(remote_player_chat_schema::created.le(&horizon))).execute(&db)
          {
            eprintln!("Failed to delete old remote player chats: {}", e);
          }
        }
      }
    }
  });
}

fn start_player_mover_task(server: &std::sync::Arc<Server>, mut spawn_receiver: tokio::sync::mpsc::Receiver<RealmMove>) {
  let s = std::sync::Arc::downgrade(server);
  tokio::spawn(async move {
    loop {
      match spawn_receiver.recv().await {
        None => break,
        Some(request) => match s.upgrade() {
          None => break,
          Some(server) => server.move_player(request).await,
        },
      }
    }
  });
}

// Actual main method. The tokio::main annotation causes all compile errors in the body to be on the line with the annotation, so keep this short
#[tokio::main(worker_threads = 8)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  start().await
}
