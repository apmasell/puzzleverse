use crate::realm::navigation::PlayerNavigationEvent;
use rmp::decode::{DecodeStringError, ValueReadError};

mod arithmetic;
mod buffer;
mod button;
mod clock;
mod comparator;
mod counter;
mod holiday;
mod index;
mod index_list;
mod logic;
mod metronome;
mod permutation;
mod proximity;
mod radio_button;
mod realm_selector;
mod sink;
mod switch;
mod timer;

/// Comparison logic between different values
pub(crate) enum Comparator {
  NotEqual,
  Equal,
  LessThanOrEqual,
  LessThan,
  GreaterThanOrEqual,
  GreaterThan,
  Congruent(u32),
}

/// A rule that determines how a puzzle piece produces player-visible state
pub(crate) struct ConsequenceRule {
  pub(crate) sender: usize,
  pub(crate) consequence: ConsequenceValueMatcher,
}

/// The logic that binds puzzle piece state values to player-visible state changes either by modifying the map or emitting a property value that will be sent to clients
pub(crate) enum ConsequenceValueMatcher {
  NumToProperty(String),
  BoolToProperty(String),
  BoolToMap(std::sync::Arc<std::sync::atomic::AtomicBool>),
  BoolToMapInverted(std::sync::Arc<std::sync::atomic::AtomicBool>),
  NumToBoolProperty { reference: u32, comparison: Comparator, property: String },
  NumToBoolMap { reference: u32, comparison: Comparator, map: std::sync::Arc<std::sync::atomic::AtomicBool> },
}

/// Read a value from a Message Pack buffer
pub(crate) trait DecodeSaved: Sized {
  /// Read the value
  fn read(input: &mut InputBuffer) -> Result<Self, rmp::decode::ValueReadError>;
}

type DeserializationResult<'a> = Result<Box<dyn PuzzlePiece + 'a>, rmp::decode::ValueReadError>;

/// A value that can be serialized
pub(crate) trait EncodeValue {
  /// Write the value to a serialization buffer
  fn write(self: &Self, output: &mut OutputBuffer) -> SerializationResult;
}

/// An event created by a puzzle piece updating its state
#[derive(Debug, Clone)]
pub(crate) struct Event {
  /// The piece that triggered the event
  pub(crate) sender: usize,
  /// The event name
  pub(crate) name: puzzleverse_core::PuzzleEvent,
  /// The output value associated with the event
  pub(crate) value: PieceValue,
}

/// A type which can be converted from a puzzle piece value
pub(crate) trait ExtractValue: Sized {
  /// Try to convert the value from a puzzle piece value
  fn extract_value(value: &PieceValue) -> Option<Self>;
}

/// A type where a list of values can be converted from a puzzle piece value
pub(crate) trait ExtractList: Sized {
  /// Try to convert a list of value from a puzzle piece value
  fn extract_list(value: &PieceValue) -> Option<&[Self]>;
}

type InputBuffer<'a> = std::io::Cursor<&'a [u8]>;

type OutputBuffer = std::io::Cursor<Vec<u8>>;

pub(crate) enum OutputEvent {
  Event(puzzleverse_core::PuzzleEvent, PieceValue),
  Send(RealmLink, Vec<crate::PlayerKey>),
}

pub(crate) type OutputEvents = Vec<OutputEvent>;

/// A value associated with an event
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PieceValue {
  Empty,
  Bool(bool),
  Num(u32),
  Realm(RealmLink),
  BoolList(Vec<bool>),
  NumList(Vec<u32>),
  RealmList(Vec<RealmLink>),
}
pub(crate) struct PropagationRule {
  pub(crate) sender: usize,
  pub(crate) trigger: puzzleverse_core::PuzzleEvent,
  pub(crate) recipient: usize,
  pub(crate) causes: puzzleverse_core::PuzzleCommand,
  pub(crate) propagation_match: PropagationValueMatcher,
}
/// Determine how a change in the internal state of a puzzle piece should be used to change the state of other puzzle pieces.
pub(crate) enum PropagationValueMatcher {
  Unchanged,
  EmptyToBool { output: bool },
  EmptyToNum { output: u32 },
  EmptyToBoolList { output: Vec<bool> },
  EmptyToNumList { output: Vec<u32> },
  EmptyToGlobalRealm { realm: String, server: String },
  EmptyToOwnerRealm { asset: String },
  EmptyToSettingRealm { setting: String },
  EmptyToSpawnPoint { name: String },
  EmptyToTrainNext,
  EmptyToHome,
  BoolInvert,
  BoolEmpty { input: bool },
  BoolToNum { input: bool, output: u32 },
  BoolToNumList { input: bool, output: Vec<u32> },
  BoolToBoolList { input: bool, output: Vec<bool> },
  NumToEmpty { input: u32, comparison: Comparator },
  NumToBool { input: u32, comparison: Comparator },
  NumToBoolList { bits: u32, low_to_high: bool },
  AnyToEmpty,
}

/// A prototype for a puzzle piece that can either produce a blank state for a new realm or recover state from a database
pub(crate) trait PuzzleAsset {
  /// Create a fresh new state
  fn create(self: &Self, time: &chrono::DateTime<chrono::Utc>) -> Box<dyn PuzzlePiece>;
  /// Recover previously serialised state
  fn load<'a>(self: &Self, input: &mut InputBuffer, time: &chrono::DateTime<chrono::Utc>) -> DeserializationResult<'a>;
}

/// The event that will have a consequence event
pub(crate) enum PuzzleConsequence<'a> {
  ClientStateUpdate(&'a str, puzzleverse_core::PropertyValue),
  UpdateMap(&'a std::sync::Arc<std::sync::atomic::AtomicBool>, bool),
}

/// An active puzzle component in a realm's logic
pub(crate) trait PuzzlePiece: Send + Sync {
  /// Adjust state based on the activity of another puzzle piece and the realm author's rules and generate any output changes the author may want to use
  fn accept(self: &mut Self, name: &puzzleverse_core::PuzzleCommand, value: &PieceValue) -> OutputEvents;
  /// Respond to a player interacting with this puzzle piece via the UI.
  ///
  /// Most puzzle pieces do not directly interact with players.
  fn interact(self: &mut Self, interaction: &puzzleverse_core::InteractionType) -> (puzzleverse_core::InteractionResult, SimpleOutputEvents);
  /// Write this puzzle piece out to a format that can be recovered later
  fn serialize(self: &Self, output: &mut OutputBuffer) -> SerializationResult;
  /// Generate any events trigger purely by time events
  fn tick(self: &mut Self, time: &chrono::DateTime<chrono::Utc>) -> SimpleOutputEvents;
  /// Determine the next time when this piece _could_ update, if it responds to clock changes
  fn next(self: &Self) -> Option<chrono::DateTime<chrono::Utc>>;
  /// Generate events when thawed from database
  fn reset(&self) -> SimpleOutputEvents;
  /// Determine if the current state of this piece should trigger any client state changes.
  ///
  /// Many pieces do not directly modify the player environment
  fn update_check<'a, 's>(self: &'s Self, state: &'a crate::puzzle::ConsequenceValueMatcher) -> Option<crate::puzzle::PuzzleConsequence<'a>>;
  /// Respond to a player entering or exiting the sphere of influence a puzzle piece
  ///
  /// Most puzzle pieces do not directly interact with player movements.
  fn walk(self: &mut Self, player: &crate::PlayerKey, event: crate::realm::navigation::PlayerNavigationEvent) -> SimpleOutputEvents;
}

/// A realm or spawn point that a player should  be sent to
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum RealmLink {
  Global(String, String),
  Owner(String),
  Spawn(String),
  Home,
  TrainNext,
}

type SerializationResult = Result<(), rmp::encode::ValueWriteError>;

type SimpleEvent = (puzzleverse_core::PuzzleEvent, PieceValue);
pub(crate) type SimpleOutputEvents = Vec<SimpleEvent>;

impl Comparator {
  /// Compare two integer values
  pub(crate) fn compare(self: &Comparator, source: u32, reference: u32) -> bool {
    match self {
      Comparator::NotEqual => source != reference,
      Comparator::Equal => source == reference,
      Comparator::LessThanOrEqual => source <= reference,
      Comparator::LessThan => source < reference,
      Comparator::GreaterThanOrEqual => source >= reference,
      Comparator::GreaterThan => source > reference,
      Comparator::Congruent(by) => (source % by) == (reference % by),
    }
  }
}

impl ConsequenceValueMatcher {
  /// Determine if there is a consequence event from this value
  pub(crate) fn apply(&self, input: &PieceValue) -> Option<PuzzleConsequence> {
    match self {
      ConsequenceValueMatcher::NumToProperty(property) => match input {
        PieceValue::Num(v) => Some(PuzzleConsequence::ClientStateUpdate(property, puzzleverse_core::PropertyValue::Num(*v))),
        _ => None,
      },
      ConsequenceValueMatcher::BoolToProperty(property) => match input {
        PieceValue::Bool(v) => Some(PuzzleConsequence::ClientStateUpdate(property, puzzleverse_core::PropertyValue::Bool(*v))),
        _ => None,
      },
      ConsequenceValueMatcher::BoolToMap(tile) => match input {
        PieceValue::Bool(v) => Some(PuzzleConsequence::UpdateMap(tile, *v)),
        _ => None,
      },
      ConsequenceValueMatcher::BoolToMapInverted(tile) => match input {
        PieceValue::Bool(v) => Some(PuzzleConsequence::UpdateMap(tile, !*v)),
        _ => None,
      },
      ConsequenceValueMatcher::NumToBoolProperty { reference, comparison, property } => match input {
        PieceValue::Num(v) => {
          Some(PuzzleConsequence::ClientStateUpdate(property, puzzleverse_core::PropertyValue::Bool(comparison.compare(*v, *reference))))
        }
        _ => None,
      },
      ConsequenceValueMatcher::NumToBoolMap { reference, comparison, map } => match input {
        PieceValue::Num(v) => Some(PuzzleConsequence::UpdateMap(map, comparison.compare(*v, *reference))),
        _ => None,
      },
    }
  }
}

impl Event {
  /// Create a new event generated by a puzzle piece
  pub(crate) fn new(sender: usize, event: SimpleEvent) -> Event {
    Event { sender, name: event.0, value: event.1 }
  }
}

impl PropagationValueMatcher {
  /// Determine the value that should be sent to the next piece based on an event's value and a rule to modify them
  pub(crate) fn apply(
    self: &PropagationValueMatcher,
    input: &PieceValue,
    settings: &std::collections::BTreeMap<String, puzzleverse_core::RealmSetting>,
  ) -> Option<PieceValue> {
    match self {
      PropagationValueMatcher::Unchanged => Some(input.clone()),
      PropagationValueMatcher::EmptyToBool { output } => match input {
        PieceValue::Empty => Some(PieceValue::Bool(*output)),
        _ => None,
      },
      PropagationValueMatcher::EmptyToNum { output } => match input {
        PieceValue::Empty => Some(PieceValue::Num(*output)),
        _ => None,
      },
      PropagationValueMatcher::EmptyToBoolList { output } => match input {
        PieceValue::Empty => Some(PieceValue::BoolList(output.clone())),
        _ => None,
      },
      PropagationValueMatcher::EmptyToNumList { output } => match input {
        PieceValue::Empty => Some(PieceValue::NumList(output.clone())),
        _ => None,
      },
      PropagationValueMatcher::EmptyToGlobalRealm { realm, server } => match input {
        PieceValue::Empty => Some(PieceValue::Realm(RealmLink::Global(realm.clone(), server.clone()))),
        _ => None,
      },
      PropagationValueMatcher::EmptyToOwnerRealm { asset } => match input {
        PieceValue::Empty => Some(PieceValue::Realm(RealmLink::Owner(asset.clone()))),
        _ => None,
      },
      PropagationValueMatcher::EmptyToSettingRealm { setting } => match input {
        PieceValue::Empty => match settings.get(setting) {
          Some(puzzleverse_core::RealmSetting::Realm { realm, server }) => Some(PieceValue::Realm(RealmLink::Global(realm.clone(), server.clone()))),
          _ => None,
        },
        _ => None,
      },
      PropagationValueMatcher::EmptyToSpawnPoint { name } => match input {
        PieceValue::Empty => Some(PieceValue::Realm(RealmLink::Spawn(name.clone()))),
        _ => None,
      },
      PropagationValueMatcher::EmptyToTrainNext => match input {
        PieceValue::Empty => Some(PieceValue::Realm(RealmLink::TrainNext)),
        _ => None,
      },
      PropagationValueMatcher::EmptyToHome => match input {
        PieceValue::Empty => Some(PieceValue::Realm(RealmLink::Home)),
        _ => None,
      },
      PropagationValueMatcher::BoolInvert => match input {
        PieceValue::Bool(input_boolean) => Some(PieceValue::Bool(!*input_boolean)),
        _ => None,
      },
      PropagationValueMatcher::BoolEmpty { input: reference } => match input {
        PieceValue::Bool(input_boolean) => {
          if input_boolean == reference {
            Some(PieceValue::Empty)
          } else {
            None
          }
        }
        _ => None,
      },
      PropagationValueMatcher::BoolToNum { input: reference, output } => match input {
        PieceValue::Bool(input_boolean) => {
          if input_boolean == reference {
            Some(PieceValue::Num(*output))
          } else {
            None
          }
        }
        _ => None,
      },
      PropagationValueMatcher::BoolToNumList { input: reference, output } => match input {
        PieceValue::Bool(input_boolean) => {
          if *input_boolean == *reference {
            Some(PieceValue::NumList(output.clone()))
          } else {
            None
          }
        }
        _ => None,
      },
      PropagationValueMatcher::BoolToBoolList { input: reference, output } => match input {
        PieceValue::Bool(input_boolean) => {
          if *input_boolean == *reference {
            Some(PieceValue::BoolList(output.clone()))
          } else {
            None
          }
        }
        _ => None,
      },
      PropagationValueMatcher::NumToEmpty { input: reference, comparison } => match input {
        PieceValue::Num(input_int) => {
          if comparison.compare(*input_int, *reference) {
            Some(PieceValue::Empty)
          } else {
            None
          }
        }
        _ => None,
      },
      PropagationValueMatcher::NumToBool { input: reference, comparison } => match input {
        PieceValue::Num(input_int) => Some(PieceValue::Bool(comparison.compare(*input_int, *reference))),
        _ => None,
      },
      PropagationValueMatcher::NumToBoolList { bits, low_to_high } => match input {
        PieceValue::Num(input_int) => Some(PieceValue::BoolList(if *low_to_high {
          (0..*bits).map(|i| *input_int & (1 << i) == 1).collect()
        } else {
          (0..*bits).rev().map(|i| *input_int & (1 << i) == 1).collect()
        })),
        _ => None,
      },
      PropagationValueMatcher::AnyToEmpty => Some(PieceValue::Empty),
    }
  }
}
/// Assert that an MessagePack value is an array length with the value specified
pub fn check_length(input: &mut InputBuffer, expected_length: u32) -> Result<(), rmp::decode::ValueReadError> {
  if rmp::decode::read_array_len(input)? == expected_length {
    Ok(())
  } else {
    Err(rmp::decode::ValueReadError::TypeMismatch(rmp::Marker::Reserved))
  }
}
/// Update a realm's state from puzzle pieces
pub(crate) fn prepare_consequences(state: &mut crate::realm::RealmPuzzleState, owner: &str, server: &std::sync::Arc<crate::Server>) -> bool {
  let mut changed = false;
  for (i, p) in state.pieces.iter().enumerate() {
    for cr in state.consequence_rules.iter().filter(|cr| cr.sender == i) {
      for c in p.update_check(&cr.consequence) {
        match c {
          PuzzleConsequence::ClientStateUpdate(name, value) => {
            changed |= state.current_states.insert(name.into(), value.clone()).map(|old| old == value).unwrap_or(true);
          }
          PuzzleConsequence::UpdateMap(storage, state) => storage.store(state, std::sync::atomic::Ordering::Relaxed),
        }
      }
    }
  }
  changed
}

/// Update puzzle state based on some player-initiated input event
pub(crate) fn process<T: IntoIterator<Item = Event>>(
  state: &mut crate::realm::RealmPuzzleState,
  links: &mut std::collections::HashMap<crate::PlayerKey, RealmLink>,
  events: T,
) {
  let mut count = 100;
  let mut queue = std::collections::VecDeque::new();
  let mut players_in_transit = Vec::new();
  queue.extend(events);
  // We only do 100 rounds of processing and then we assume the puzzle is some kind of unstable oscillator and give up.
  while count > 0 {
    count -= 1;
    // We're really handling two kinds of events: one based on our input and then knock on effects from having moved players around, so first, drain all our input events
    if let Some(current) = queue.pop_front() {
      // Check each puzzle piece for an input event and gather the output events
      for propagation_rule in state
        .propagation_rules
        .iter()
        .filter(|propagation_rule| propagation_rule.sender == current.sender && propagation_rule.trigger == current.name)
      {
        if let Some(piece) = state.pieces.get_mut(propagation_rule.recipient) {
          if let Some(result) = propagation_rule.propagation_match.apply(&current.value, &state.settings).as_mut() {
            for value in piece.accept(&propagation_rule.causes, result) {
              match value {
                // Any output events that are due to puzzle state, get fed back into out input queue
                OutputEvent::Event(event_name, value) => {
                  queue.push_back(Event { sender: propagation_rule.recipient, name: event_name, value });
                }
                // The puzzle piece wants to move players; if we've already moved the players around, don't move them again
                OutputEvent::Send(link, players) => {
                  for player in players {
                    if !links.contains_key(&player) {
                      players_in_transit.push(player.clone());
                      links.insert(player, link.clone());
                    }
                  }
                }
              }
            }
          }
        }
      }
    } else if !players_in_transit.is_empty() {
      // Given we've processed all the input events, then make sure our players that are leaving exist nowhere on the map and all the pieces no that. If that generates more events, put them in the queue
      for player in players_in_transit.drain(..) {
        for (recipient, piece) in state.pieces.iter_mut().enumerate() {
          queue.extend(piece.walk(&player, PlayerNavigationEvent::Leave).drain(..).map(|(event_name, value)| Event {
            sender: recipient,
            name: event_name,
            value,
          }));
        }
      }
    } else {
      // This means we are done with input events and done with clearing players on the map
      break;
    }
  }
}

/// Update puzzle piece states based on the tick of the clock
pub(crate) fn process_time(
  state: &mut crate::realm::RealmPuzzleState,
  time: &chrono::DateTime<chrono::Utc>,
  links: &mut std::collections::HashMap<crate::PlayerKey, RealmLink>,
) {
  let mut events = Vec::new();
  for (piece_name, piece) in state.pieces.iter_mut().enumerate() {
    events.extend(piece.tick(time).drain(..).map::<Event, _>(|(name, value)| Event { sender: piece_name, name, value }))
  }
  process(state, links, events.drain(..))
}

/// Read a string value from a buffer
fn read_str(input: &mut InputBuffer) -> Result<String, ValueReadError> {
  rmp::decode::read_str(input, &mut Vec::new())
    .map_err(|err| match err {
      rmp::decode::DecodeStringError::InvalidMarkerRead(err) => ValueReadError::InvalidMarkerRead(err),
      rmp::decode::DecodeStringError::InvalidDataRead(err) => ValueReadError::InvalidDataRead(err),
      rmp::decode::DecodeStringError::TypeMismatch(marker) => ValueReadError::TypeMismatch(marker),
      DecodeStringError::BufferSizeTooSmall(_) => ValueReadError::TypeMismatch(rmp::Marker::Str8),
      DecodeStringError::InvalidUtf8(_, _) => ValueReadError::TypeMismatch(rmp::Marker::Str8),
    })
    .map(|s| s.to_string())
}
impl Into<PieceValue> for bool {
  fn into(self: Self) -> PieceValue {
    PieceValue::Bool(self)
  }
}

impl Into<PieceValue> for Vec<bool> {
  fn into(self: Self) -> PieceValue {
    PieceValue::BoolList(self)
  }
}
impl DecodeSaved for bool {
  fn read(input: &mut InputBuffer) -> Result<Self, rmp::decode::ValueReadError> {
    rmp::decode::read_bool(input)
  }
}

impl EncodeValue for bool {
  fn write(self: &Self, output: &mut OutputBuffer) -> SerializationResult {
    rmp::encode::write_bool(output, *self).map_err(rmp::encode::ValueWriteError::InvalidDataWrite)
  }
}

impl ExtractList for bool {
  fn extract_list(value: &PieceValue) -> Option<&[bool]> {
    match value {
      PieceValue::BoolList(v) => Some(v),
      _ => None,
    }
  }
}

impl ExtractValue for bool {
  fn extract_value(value: &PieceValue) -> Option<bool> {
    match value {
      PieceValue::Bool(v) => Some(*v),
      _ => None,
    }
  }
}

impl Into<PieceValue> for u32 {
  fn into(self: Self) -> PieceValue {
    PieceValue::Num(self)
  }
}

impl Into<PieceValue> for Vec<u32> {
  fn into(self: Self) -> PieceValue {
    PieceValue::NumList(self)
  }
}

impl DecodeSaved for u32 {
  fn read(input: &mut InputBuffer) -> Result<Self, ValueReadError> {
    rmp::decode::read_u32(input)
  }
}

impl EncodeValue for u32 {
  fn write(self: &Self, output: &mut OutputBuffer) -> SerializationResult {
    rmp::encode::write_u32(output, *self)
  }
}

impl ExtractList for u32 {
  fn extract_list(value: &PieceValue) -> Option<&[u32]> {
    match value {
      PieceValue::NumList(v) => Some(v),
      _ => None,
    }
  }
}

impl Into<PieceValue> for Vec<RealmLink> {
  fn into(self: Self) -> PieceValue {
    PieceValue::RealmList(self)
  }
}

impl ExtractValue for u32 {
  fn extract_value(value: &PieceValue) -> Option<u32> {
    match value {
      PieceValue::Num(v) => Some(*v),
      _ => None,
    }
  }
}

impl Into<PieceValue> for RealmLink {
  fn into(self: Self) -> PieceValue {
    PieceValue::Realm(self)
  }
}

impl DecodeSaved for RealmLink {
  fn read(input: &mut InputBuffer) -> Result<Self, ValueReadError> {
    check_length(input, 2)?;
    match rmp::decode::read_u8(input)? {
      b'g' => {
        check_length(input, 2)?;
        Ok(RealmLink::Global(read_str(input)?.into(), read_str(input)?.into()))
      }
      b's' => Ok(RealmLink::Spawn(read_str(input)?.into())),
      b'o' => Ok(RealmLink::Owner(read_str(input)?.into())),
      b'h' => Ok(RealmLink::Home),
      b't' => Ok(RealmLink::TrainNext),
      _ => Err(ValueReadError::TypeMismatch(rmp::Marker::Reserved)),
    }
  }
}

impl EncodeValue for RealmLink {
  fn write(self: &Self, output: &mut OutputBuffer) -> SerializationResult {
    rmp::encode::write_array_len(output, 2)?;
    match self {
      RealmLink::Global(realm, server) => {
        rmp::encode::write_u8(output, b'g')?;
        rmp::encode::write_array_len(output, 2)?;
        rmp::encode::write_str(output, realm)?;
        rmp::encode::write_str(output, server)
      }
      RealmLink::Owner(name) => {
        rmp::encode::write_u8(output, b'o')?;
        rmp::encode::write_str(output, name)
      }
      RealmLink::Spawn(point) => {
        rmp::encode::write_u8(output, b's')?;
        rmp::encode::write_str(output, point)
      }
      RealmLink::Home => rmp::encode::write_u8(output, b'h'),
      RealmLink::TrainNext => rmp::encode::write_u8(output, b't'),
    }
  }
}

impl ExtractList for RealmLink {
  fn extract_list(value: &PieceValue) -> Option<&[RealmLink]> {
    match value {
      PieceValue::RealmList(v) => Some(v),
      _ => None,
    }
  }
}

impl ExtractValue for RealmLink {
  fn extract_value(value: &PieceValue) -> Option<RealmLink> {
    match value {
      PieceValue::Realm(v) => Some(v.clone()),
      _ => None,
    }
  }
}
