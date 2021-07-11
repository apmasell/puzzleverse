use crate::puzzle::OutputEvent;
use rand::Rng;
const SERIALIZATION_LENGTH: u32 = 0;

struct Proximity {
  players: std::collections::HashSet<crate::PlayerKey>,
}

struct ProximityAsset {}

impl crate::puzzle::PuzzleAsset for ProximityAsset {
  fn create(self: &Self, _: &chrono::DateTime<chrono::Utc>) -> Box<dyn crate::puzzle::PuzzlePiece> {
    Box::new(Proximity { players: std::collections::HashSet::new() }) as Box<dyn crate::puzzle::PuzzlePiece>
  }
  fn load<'a>(self: &Self, input: &mut crate::puzzle::InputBuffer, _: &chrono::DateTime<chrono::Utc>) -> crate::puzzle::DeserializationResult<'a> {
    crate::puzzle::check_length(input, SERIALIZATION_LENGTH)?;
    Ok(Box::new(Proximity { players: std::collections::HashSet::new() }) as Box<dyn crate::puzzle::PuzzlePiece>)
  }
}

impl crate::puzzle::PuzzlePiece for Proximity {
  fn accept(self: &mut Self, name: &puzzleverse_core::PuzzleCommand, value: &crate::puzzle::PieceValue) -> crate::puzzle::OutputEvents {
    if name == &puzzleverse_core::PuzzleCommand::Send {
      match value {
        crate::puzzle::PieceValue::Realm(realm) => {
          let players = self.players.drain().collect();
          vec![
            crate::puzzle::OutputEvent::Send(realm.clone(), players),
            crate::puzzle::OutputEvent::Event(puzzleverse_core::PuzzleEvent::Changed, (self.players.len() as u32).into()),
          ]
        }
        crate::puzzle::PieceValue::RealmList(realms) => {
          if realms.is_empty() {
            vec![]
          } else {
            let mut sends: Vec<OutputEvent> = self
              .players
              .drain()
              .flat_map(|p| {
                Some(crate::puzzle::OutputEvent::Send(realms.get(rand::thread_rng().gen_range(0..realms.len()))?.clone(), vec![p.clone()]))
              })
              .collect();
            sends.push(crate::puzzle::OutputEvent::Event(puzzleverse_core::PuzzleEvent::Changed, (self.players.len() as u32).into()));
            sends
          }
        }
        _ => vec![],
      }
    } else {
      vec![]
    }
  }
  fn interact(
    self: &mut Self,
    _: &puzzleverse_core::InteractionType,
    _: &str,
  ) -> (puzzleverse_core::InteractionResult, crate::puzzle::SimpleOutputEvents) {
    (puzzleverse_core::InteractionResult::Invalid, vec![])
  }

  fn serialize(self: &Self, output: &mut crate::puzzle::OutputBuffer) -> crate::puzzle::SerializationResult {
    rmp::encode::write_array_len(output, SERIALIZATION_LENGTH)?;
    Ok(())
  }
  fn tick(self: &mut Self, _: &chrono::DateTime<chrono::Utc>) -> crate::puzzle::SimpleOutputEvents {
    vec![]
  }
  fn reset(&self) -> crate::puzzle::SimpleOutputEvents {
    vec![(puzzleverse_core::PuzzleEvent::Changed, (self.players.len() as u32).into())]
  }
  fn next(self: &Self) -> Option<chrono::DateTime<chrono::Utc>> {
    None
  }
  fn update_check<'a, 's>(self: &'s Self, _: &'a crate::puzzle::ConsequenceValueMatcher) -> Option<crate::puzzle::PuzzleConsequence<'a>> {
    None
  }
  fn walk(self: &mut Self, player: &crate::PlayerKey, event: crate::realm::navigation::PlayerNavigationEvent) -> crate::puzzle::SimpleOutputEvents {
    let old_size = self.players.len();
    match event {
      crate::realm::navigation::PlayerNavigationEvent::Enter => {
        self.players.insert(player.clone());
      }
      crate::realm::navigation::PlayerNavigationEvent::Leave => {
        self.players.remove(&player);
      }
    }
    if old_size == self.players.len() {
      vec![]
    } else {
      vec![(puzzleverse_core::PuzzleEvent::Changed, (self.players.len() as u32).into())]
    }
  }
}
