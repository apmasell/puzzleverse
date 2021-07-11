use crate::puzzle::{DecodeSaved, EncodeValue, PuzzlePiece};
const SERIALIZATION_LENGTH: u32 = 2;

pub(crate) struct RealmSelector {
  realm: crate::puzzle::RealmLink,
}

pub(crate) struct RealmSelectorAsset {}

impl crate::puzzle::PuzzleAsset for RealmSelectorAsset {
  fn create(self: &Self, _: &chrono::DateTime<chrono::Utc>) -> Box<dyn crate::puzzle::PuzzlePiece> {
    std::boxed::Box::new(RealmSelector { realm: crate::puzzle::RealmLink::Home })
  }
  fn load<'a>(self: &Self, input: &mut crate::puzzle::InputBuffer, _: &chrono::DateTime<chrono::Utc>) -> crate::puzzle::DeserializationResult<'a> {
    crate::puzzle::check_length(input, SERIALIZATION_LENGTH)?;
    Ok(Box::new(RealmSelector { realm: crate::puzzle::RealmLink::read(input)? }) as Box<dyn PuzzlePiece>)
  }
}

impl PuzzlePiece for RealmSelector {
  fn accept(self: &mut Self, _: &puzzleverse_core::PuzzleCommand, _: &crate::puzzle::PieceValue) -> crate::puzzle::OutputEvents {
    vec![]
  }
  fn interact(
    self: &mut Self,
    interaction: &puzzleverse_core::InteractionType,
    player_server: &str,
  ) -> (puzzleverse_core::InteractionResult, crate::puzzle::SimpleOutputEvents) {
    if let puzzleverse_core::InteractionType::Realm(realm) = interaction {
      (
        puzzleverse_core::InteractionResult::Accepted,
        vec![(
          puzzleverse_core::PuzzleEvent::Changed,
          crate::puzzle::PieceValue::Realm(match realm {
            puzzleverse_core::RealmTarget::Home => crate::puzzle::RealmLink::Home,
            puzzleverse_core::RealmTarget::RemoteRealm { realm, server } => crate::puzzle::RealmLink::Global(realm.clone(), server.clone()),
            puzzleverse_core::RealmTarget::PersonalRealm(asset) => crate::puzzle::RealmLink::Owner(asset.clone()),
            puzzleverse_core::RealmTarget::LocalRealm(realm) => crate::puzzle::RealmLink::Global(realm.clone(), player_server.to_string()),
          }),
        )],
      )
    } else {
      (puzzleverse_core::InteractionResult::Invalid, vec![])
    }
  }

  fn serialize(self: &Self, output: &mut crate::puzzle::OutputBuffer) -> crate::puzzle::SerializationResult {
    rmp::encode::write_array_len(output, SERIALIZATION_LENGTH)?;
    self.realm.write(output)
  }
  fn tick(self: &mut Self, _: &chrono::DateTime<chrono::Utc>) -> crate::puzzle::SimpleOutputEvents {
    vec![]
  }
  fn next(self: &Self) -> Option<chrono::DateTime<chrono::Utc>> {
    None
  }
  fn reset(&self) -> crate::puzzle::SimpleOutputEvents {
    vec![]
  }
  fn update_check<'a, 's>(self: &'s Self, _: &'a crate::puzzle::ConsequenceValueMatcher) -> Option<crate::puzzle::PuzzleConsequence<'a>> {
    None
  }
  fn walk(self: &mut Self, _: &crate::PlayerKey, _: crate::realm::navigation::PlayerNavigationEvent) -> crate::puzzle::SimpleOutputEvents {
    vec![]
  }
}
