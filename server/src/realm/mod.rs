use diesel::prelude::*;

pub(crate) mod navigation;

pub(crate) enum LoadError {
  R2D2(r2d2::Error),
  Diesel(diesel::result::Error),
  Serde(rmp_serde::decode::Error),
  Rmp(rmp::decode::ValueReadError),
  NoOwner,
  BadRealm,
}

#[derive(Clone)]
pub(crate) struct PlayerMovement {
  pub(crate) player: crate::PlayerKey,
  pub(crate) movement: puzzleverse_core::CharacterMotion<puzzleverse_core::Point>,
  pub(crate) leave_pieces: Vec<usize>,
  pub(crate) enter_pieces: Vec<usize>,
}

pub(crate) struct RealmPuzzleState {
  pub(crate) active_players: std::collections::HashMap<crate::PlayerKey, (puzzleverse_core::Point, Vec<puzzleverse_core::Action>)>,
  pub(crate) committed_movements: Vec<PlayerMovement>,
  pub(crate) consequence_rules: Vec<crate::puzzle::ConsequenceRule>,
  pub(crate) current_states: puzzleverse_core::PropertyStates,
  pub(crate) last_update: chrono::DateTime<chrono::Utc>,
  pub(crate) manifold: crate::realm::navigation::RealmManifold,
  pub(crate) pieces: Vec<Box<dyn crate::puzzle::PuzzlePiece>>,
  pub(crate) propagation_rules: Vec<crate::puzzle::PropagationRule>,
  pub(crate) settings: std::collections::BTreeMap<String, puzzleverse_core::RealmSetting>,
}

/// Information about an active realm
#[derive(Clone)]
pub(crate) struct RealmState {
  pub(crate) access_acl: std::sync::Arc<tokio::sync::Mutex<crate::AccessControlSetting>>,
  pub(crate) admin_acl: std::sync::Arc<tokio::sync::Mutex<crate::AccessControlSetting>>,
  pub(crate) asset: String,
  pub(crate) capabilities: Vec<String>,
  pub(crate) consent_epoch: std::sync::Arc<std::sync::atomic::AtomicI32>,
  pub(crate) db_id: i32,
  pub(crate) id: String,
  pub(crate) in_directory: std::sync::Arc<tokio::sync::RwLock<bool>>,
  pub(crate) name: std::sync::Arc<tokio::sync::RwLock<String>>,
  pub(crate) owner: String,
  pub(crate) puzzle_state: std::sync::Arc<tokio::sync::Mutex<RealmPuzzleState>>,
  pub(crate) seed: i32,
}
impl RealmPuzzleState {
  /// Trigger a player interaction
  async fn interact(
    &mut self,
    server: &std::sync::Arc<crate::Server>,
    realm_owner: &str,
    target: usize,
    interaction: &puzzleverse_core::InteractionType,
    player_server: &str,
    time: &chrono::DateTime<chrono::Utc>,
    train: Option<u16>,
  ) -> puzzleverse_core::InteractionResult {
    if let Some(piece) = self.pieces.get_mut(target) {
      let (result, events) = piece.interact(interaction, player_server);
      let mut links = std::collections::HashMap::new();
      crate::puzzle::process(&mut *self, &mut links, events.into_iter().map(|event| crate::puzzle::Event::new(target, event)));
      for (p, rl) in links.iter() {
        match rl {
          crate::puzzle::RealmLink::Spawn(point) => match self.manifold.warp(Some(point)) {
            Some(point) => {
              if let Some((current_position, _)) = self.active_players.get(p) {
                self.committed_movements.retain(|m| m.player != *p || m.movement.time() < time);
                self.committed_movements.push(PlayerMovement {
                  player: p.clone(),
                  leave_pieces: self.manifold.active_proximity(current_position).collect(),
                  enter_pieces: vec![],
                  movement: puzzleverse_core::CharacterMotion::Leave { from: (*current_position).clone(), start: *time },
                });
                self.committed_movements.push(PlayerMovement {
                  player: p.clone(),
                  leave_pieces: vec![],
                  enter_pieces: self.manifold.active_proximity(&point).collect(),
                  movement: puzzleverse_core::CharacterMotion::Enter {
                    to: point,
                    end: *time + chrono::Duration::milliseconds(navigation::WARP_TIME.into()),
                  },
                });
              }
            }
            None => (),
          },
          _ => (),
        }
      }
      links.retain(|_, v| !matches!(v, crate::puzzle::RealmLink::Spawn(_)));
      server.move_players_from_realm(realm_owner, train, links).await;
      result
    } else {
      puzzleverse_core::InteractionResult::Invalid
    }
  }
  pub(crate) fn make_update_state(
    &self,
    players: &slotmap::DenseSlotMap<crate::PlayerKey, crate::player_state::PlayerState>,
  ) -> puzzleverse_core::RealmResponse {
    let mut movements = std::collections::HashMap::new();
    for m in &self.committed_movements {
      if let Some(player_name) = players.get(m.player) {
        match movements.entry(player_name.principal.clone()) {
          std::collections::hash_map::Entry::Vacant(entry) => {
            entry.insert(vec![m.movement.clone()]);
          }
          std::collections::hash_map::Entry::Occupied(mut entry) => {
            entry.get_mut().push(m.movement.clone());
          }
        }
      }
    }
    let states = self.current_states.clone();
    puzzleverse_core::RealmResponse::UpdateState { state: states, player: movements }
  }
  /// Process an event sent by a client
  pub async fn process_realm_event(
    &mut self,
    server: &std::sync::Arc<crate::Server>,
    db_id: i32,
    mut active_player: Option<(&crate::PlayerKey, &mut crate::player_state::MutablePlayerState)>,
    message: puzzleverse_core::RealmResponse,
  ) {
    match &message {
      puzzleverse_core::RealmResponse::NameChanged(name, in_directory) => {
        use crate::schema::realm::dsl as realm_schema;
        let db_connection = server.db_pool.get().unwrap();
        if let Err(e) = diesel::update(realm_schema::realm.filter(realm_schema::id.eq(db_id)))
          .set((realm_schema::name.eq(&name), realm_schema::in_directory.eq(in_directory)))
          .execute(&db_connection)
        {
          println!("Failed to update realm name: {}", e);
        }
      }
      puzzleverse_core::RealmResponse::SettingChanged(_, _) => {
        use crate::schema::realm::dsl as realm_schema;
        let db_connection = server.db_pool.get().unwrap();
        match rmp_serde::to_vec(&self.settings) {
          Ok(settings) => {
            if let Err(e) =
              diesel::update(realm_schema::realm.filter(realm_schema::id.eq(db_id))).set(realm_schema::settings.eq(&settings)).execute(&db_connection)
            {
              println!("Failed to update realm settings: {}", e);
            }
          }
          Err(e) => println!("Failed to update realm settings: {}", e),
        }
      }
      puzzleverse_core::RealmResponse::MessagePosted { body, sender, timestamp } => {
        use crate::schema::realmchat::dsl as realm_chat_schema;

        let db_connection = server.db_pool.get().unwrap();
        if let Err(e) = diesel::insert_into(realm_chat_schema::realmchat)
          .values((
            realm_chat_schema::body.eq(&body),
            realm_chat_schema::principal.eq(&sender),
            realm_chat_schema::created.eq(&timestamp),
            realm_chat_schema::realm.eq(db_id),
          ))
          .execute(&db_connection)
        {
          println!("Failed to update realm name: {}", e);
        }
      }
      _ => (),
    }
    for player in self.active_players.keys() {
      if active_player.as_ref().map(|(id, _)| *id == player).unwrap_or(false) {
        active_player.as_mut().unwrap().1.connection.send(server, player, message.clone()).await;
      } else if let Some(state) = server.player_states.read().await.get(player.clone()) {
        state.mutable.lock().await.connection.send(server, player, message.clone()).await
      }
    }
  }
  pub(crate) fn reset(&mut self) {
    let events: Vec<_> = self
      .pieces
      .iter()
      .enumerate()
      .flat_map(|(sender, piece)| piece.reset().into_iter().map(move |event| crate::puzzle::Event::new(sender, event)))
      .collect();
    let mut links = std::collections::HashMap::new();
    crate::puzzle::process(&mut *self, &mut links, events);
    // We can safely throw links away because there are no players to move anywhere.
    if !links.is_empty() {
      eprintln!("Realm wants to move players after reset. It should have no players");
    }
  }
  /// Insert a new player into this realm
  pub(crate) async fn spawn_player(
    &mut self,
    server: &crate::Server,
    realm_name: &str,
    in_directory: bool,
    realm_key: &crate::RealmKey,
    player_key: &crate::PlayerKey,
    player_state: &mut crate::player_state::MutablePlayerState,
  ) {
    match self.manifold.warp(None) {
      Some(point) => {
        player_state.goal = crate::player_state::Goal::InRealm(realm_key.clone(), crate::player_state::RealmGoal::Idle);
        self.active_players.insert(player_key.clone(), (point.clone(), vec![]));
        self.committed_movements.push(crate::realm::PlayerMovement {
          player: player_key.clone(),
          leave_pieces: vec![],
          enter_pieces: self.manifold.active_proximity(&point).collect(),
          movement: puzzleverse_core::CharacterMotion::Enter { to: point, end: chrono::Utc::now() },
        });
        let players = server.player_states.read().await;
        player_state.connection.send(server, player_key, puzzleverse_core::RealmResponse::NameChanged(realm_name.to_string(), in_directory)).await;
        player_state.connection.send(server, player_key, self.make_update_state(&players)).await;
      }
      None => {
        player_state.goal = crate::player_state::Goal::Undecided;
        self.active_players.remove(player_key);
        player_state.connection.release_player(server).await;
      }
    }
  }
  fn update_locations(&mut self, links: &mut std::collections::HashMap<crate::PlayerKey, crate::puzzle::RealmLink>) {
    let time = chrono::Utc::now();
    self.committed_movements.sort_by_key(|m| *m.movement.time());
    let possible_movements: Vec<_> =
      self.committed_movements.iter().filter(|m| m.movement.time() > &self.last_update && m.movement.time() < &time).cloned().collect();
    for m in possible_movements {
      match &m.movement {
        puzzleverse_core::CharacterMotion::Leave { .. } => {
          self.active_players.remove(&m.player);
        }
        puzzleverse_core::CharacterMotion::Enter { to, .. } => {
          self.active_players.insert(m.player.clone(), ((*to).clone(), vec![]));
        }
        puzzleverse_core::CharacterMotion::Internal { to, .. } => {
          self.active_players.insert(m.player.clone(), ((*to).clone(), vec![]));
        }
        puzzleverse_core::CharacterMotion::Interaction { at, .. } => {
          self.active_players.insert(m.player.clone(), ((*at).clone(), vec![]));
        }
        puzzleverse_core::CharacterMotion::DirectedEmote { at, .. } => {
          self.active_players.insert(m.player.clone(), ((*at).clone(), vec![]));
        }
        puzzleverse_core::CharacterMotion::ConsensualEmoteInitiator { at, .. } => {
          self.active_players.insert(m.player.clone(), ((*at).clone(), vec![]));
        }
        puzzleverse_core::CharacterMotion::ConsensualEmoteRecipient { at, .. } => {
          self.active_players.insert(m.player.clone(), ((*at).clone(), vec![]));
        }
      }
      for (&target, event) in m
        .leave_pieces
        .iter()
        .zip(std::iter::repeat(crate::realm::navigation::PlayerNavigationEvent::Leave))
        .chain(m.enter_pieces.iter().zip(std::iter::repeat(crate::realm::navigation::PlayerNavigationEvent::Enter)))
      {
        let move_events = match self.pieces.get_mut(target) {
          Some(piece) => piece
            .walk(&m.player, event)
            .into_iter()
            .map(|(event_name, value)| crate::puzzle::Event { sender: target, name: event_name, value })
            .collect(),
          None => vec![],
        };
        crate::puzzle::process(self, links, move_events.into_iter());
      }
    }
    self.committed_movements.retain(|m| m.movement.time() < &(time - chrono::Duration::seconds(10)));
    self.last_update = time;
  }
  /// Forcibly extract a player from this realm
  pub(crate) fn yank(&mut self, player_id: &crate::PlayerKey, links: &mut std::collections::HashMap<crate::PlayerKey, crate::puzzle::RealmLink>) {
    if let Some((point, _)) = self.active_players.remove(player_id) {
      let time = chrono::Utc::now();
      self.committed_movements.retain(|m| m.player != *player_id || m.movement.time() < &time);
      self.committed_movements.push(crate::realm::PlayerMovement {
        player: player_id.clone(),
        movement: puzzleverse_core::CharacterMotion::Leave { from: point.clone(), start: time },
        leave_pieces: self.manifold.active_proximity(&point).collect(),
        enter_pieces: vec![],
      });
      let mut output_events = Vec::new();
      for target in self.manifold.active_proximity(&point) {
        match self.pieces.get_mut(target) {
          Some(piece) => output_events.extend(
            piece
              .walk(player_id, crate::realm::navigation::PlayerNavigationEvent::Leave)
              .into_iter()
              .map(|(event_name, value)| crate::puzzle::Event { sender: target, name: event_name, value }),
          ),
          None => (),
        }
      }

      crate::puzzle::process(&mut *self, links, output_events.into_iter());
    }
  }
}
impl RealmState {
  /// Load a realm from the database
  pub(crate) async fn load(server: &std::sync::Arc<crate::Server>, principal: &str) -> Result<RealmState, LoadError> {
    use crate::schema::player::dsl as player_schema;
    use crate::schema::realm::dsl as realm_schema;

    let db_connection = server.db_pool.get().map_err(LoadError::R2D2)?;

    let (db_id, initialised, admin_acls_bin, access_acls_bin, puzzle_state_bin, seed, settings_bin, asset, name, in_directory, owner, train): (
      i32,
      bool,
      Vec<u8>,
      Vec<u8>,
      Vec<u8>,
      i32,
      Vec<u8>,
      String,
      String,
      bool,
      Option<String>,
      Option<i32>,
    ) = realm_schema::realm
      .select((
        realm_schema::id,
        realm_schema::initialised,
        realm_schema::admin_acl,
        realm_schema::access_acl,
        realm_schema::state,
        realm_schema::seed,
        realm_schema::settings,
        realm_schema::asset,
        realm_schema::name,
        realm_schema::in_directory,
        player_schema::player.select(player_schema::name).filter(player_schema::id.eq(realm_schema::owner)).single_value(),
        realm_schema::train,
      ))
      .filter(realm_schema::principal.eq(principal))
      .first(&db_connection)
      .map_err(LoadError::Diesel)?;

    let train = train.map(|t| t as u16 + 1);
    let admin_acls: crate::AccessControlSetting = rmp_serde::from_read(std::io::Cursor::new(admin_acls_bin.as_slice())).map_err(LoadError::Serde)?;
    let access_acls: crate::AccessControlSetting =
      rmp_serde::from_read(std::io::Cursor::new(access_acls_bin.as_slice())).map_err(LoadError::Serde)?;
    let (capabilities, mut puzzle_state) = server
      .load_realm_description(
        &asset,
        Err(LoadError::BadRealm),
        |capabilities, piece_assets, propagation_rules, consequence_rules, manifold, settings| {
          let now = chrono::Utc::now();
          let pieces = if initialised {
            let mut input = std::io::Cursor::new(puzzle_state_bin.as_slice());
            crate::puzzle::check_length(&mut input, piece_assets.len() as u32).map_err(LoadError::Rmp)?;

            let mut pieces = Vec::new();
            for p in piece_assets {
              pieces.push(p.load(&mut input, &now).map_err(LoadError::Rmp)?);
            }
            pieces
          } else {
            piece_assets.iter().map(|pa| pa.create(&now)).collect()
          };
          let mut state = RealmPuzzleState {
            pieces,
            propagation_rules,
            consequence_rules,
            manifold,
            active_players: std::collections::HashMap::new(),
            committed_movements: vec![],
            current_states: std::collections::HashMap::new(),
            last_update: chrono::Utc::now(),
            settings,
          };
          crate::puzzle::prepare_consequences(&mut state, owner.as_ref().unwrap(), server);
          Ok((capabilities, state))
        },
      )
      .await?;
    for (name, saved_setting) in
      rmp_serde::from_read::<_, std::collections::BTreeMap<String, puzzleverse_core::RealmSetting>>(std::io::Cursor::new(settings_bin.as_slice()))
        .map_err(LoadError::Serde)?
    {
      if let Some(setting) = puzzle_state.settings.get_mut(&name) {
        setting.type_matched_update(&saved_setting);
      }
    }
    puzzle_state.reset();

    let puzzle_state = std::sync::Arc::new(tokio::sync::Mutex::new(puzzle_state));
    let ps = std::sync::Arc::downgrade(&puzzle_state);
    let s = server.clone();
    let realm_owner = owner.as_ref().unwrap().clone();
    // Create a process that updates the realm's internal state and flushes it to players
    tokio::spawn(async move {
      loop {
        let last = chrono::Utc::now();
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;

        if let Some(upgraded) = ps.upgrade() {
          let mut state = upgraded.lock().await;
          let now = chrono::Utc::now();
          let mut links = std::collections::HashMap::new();
          crate::puzzle::process_time(&mut *state, &now, &mut links);
          let mut changed = !links.is_empty();
          state.committed_movements.retain(|m| m.movement.time() <= &last || !links.contains_key(&m.player));

          s.move_players_from_realm(&realm_owner, train, links).await;

          let mut had_interaction = false;
          let mut bad_interactions = std::collections::HashMap::new();
          let interactions: Vec<_> = state
            .committed_movements
            .iter()
            .filter(|m| m.movement.time() > &last && m.movement.time() <= &now)
            .flat_map(|m| match &m.movement {
              puzzleverse_core::CharacterMotion::Interaction { interaction, at, start, .. } => {
                Some((m.player.clone(), interaction.clone(), at.clone(), start.clone()))
              }
              _ => None,
            })
            .collect();

          for (player, interaction, at, time) in interactions.into_iter() {
            if !bad_interactions.contains_key(&player) {
              if let Some(piece_id) = state.manifold.interaction_target(&at) {
                match state
                  .interact(
                    &s,
                    &realm_owner,
                    piece_id,
                    &interaction,
                    s.player_states.read().await.get(player.clone()).map(|p| p.server.as_ref()).flatten().unwrap_or(&s.name),
                    &now,
                    train,
                  )
                  .await
                {
                  puzzleverse_core::InteractionResult::Invalid => eprintln!("Invalid interaction on puzzle piece {} in realm {}", piece_id, db_id),
                  puzzleverse_core::InteractionResult::Failed => {
                    bad_interactions.insert(player, time);
                  }
                  puzzleverse_core::InteractionResult::Accepted => {
                    had_interaction = true;
                  }
                }
              }
            }
          }
          state.committed_movements.retain(|m| match bad_interactions.get(&m.player) {
            None => true,
            Some(time) => m.movement.time() <= time,
          });

          let mut links2 = std::collections::HashMap::new();
          state.update_locations(&mut links2);
          changed |= !links2.is_empty();
          state.committed_movements.retain(|m| m.movement.time() <= &last || !links2.contains_key(&m.player));
          s.move_players_from_realm(&realm_owner, train, links2).await;

          if crate::puzzle::prepare_consequences(&mut state, &realm_owner, &s) || changed || had_interaction {
            let players = s.player_states.read().await;
            let update = state.make_update_state(&players);
            state.process_realm_event(&s, db_id, None, update).await;
          }
          let mut cursor = std::io::Cursor::new(Vec::new());
          match rmp::encode::write_array_len(&mut cursor, state.pieces.len() as u32) {
            Ok(_) => {}
            Err(e) => eprintln!("Failed to serialise {}: {}", db_id, e),
          }
          for (piece_id, piece) in state.pieces.iter().enumerate() {
            match piece.serialize(&mut cursor) {
              Ok(_) => {}
              Err(e) => {
                eprintln!("Failed to serialise {} piece {}: {}", db_id, piece_id, e)
              }
            }
          }
          let db_connection = &s.db_pool.get().unwrap();
          diesel::update(realm_schema::realm.filter(realm_schema::id.eq(db_id)))
            .set((realm_schema::state.eq(cursor.into_inner()), realm_schema::initialised.eq(true)))
            .execute(db_connection)
            .unwrap();
        } else {
          break;
        }
      }
    });

    Ok(RealmState {
      access_acl: std::sync::Arc::new(tokio::sync::Mutex::new(access_acls)),
      admin_acl: std::sync::Arc::new(tokio::sync::Mutex::new(admin_acls)),
      asset,
      capabilities,
      consent_epoch: std::sync::Arc::new(std::sync::atomic::AtomicI32::new(0)),
      db_id,
      id: principal.to_string(),
      in_directory: std::sync::Arc::new(tokio::sync::RwLock::new(in_directory)),
      name: std::sync::Arc::new(tokio::sync::RwLock::new(name)),
      owner: format!("{}@{}", owner.ok_or(LoadError::NoOwner)?, server.name),
      puzzle_state,
      seed,
    })
  }
}
impl std::fmt::Debug for LoadError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      LoadError::R2D2(v) => v.fmt(f),
      LoadError::Diesel(v) => v.fmt(f),
      LoadError::Serde(v) => v.fmt(f),
      LoadError::Rmp(v) => v.fmt(f),
      LoadError::NoOwner => f.write_str("Realm has no owner. Missing foreign key constraint in DB?"),
      LoadError::BadRealm => f.write_str("Asset is not a realm that this server can handle."),
    }
  }
}
