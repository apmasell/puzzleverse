const SERIALIZATION_LENGTH: u32 = 1;
struct Holiday {
  calendar: std::sync::Arc<Box<dyn bdays::HolidayCalendar<chrono::DateTime<chrono::Local>> + Send + Sync>>,
  last: bool,
}

struct HolidayAsset {
  calendar: std::sync::Arc<Box<dyn bdays::HolidayCalendar<chrono::DateTime<chrono::Local>> + Send + Sync>>,
}

impl crate::puzzle::PuzzleAsset for HolidayAsset {
  fn create(self: &Self, time: &chrono::DateTime<chrono::Utc>) -> Box<dyn crate::puzzle::PuzzlePiece> {
    Box::new(Holiday { calendar: self.calendar.clone(), last: self.calendar.is_holiday(time.with_timezone(&chrono::Local)) })
      as Box<dyn crate::puzzle::PuzzlePiece>
  }
  fn load<'a>(self: &Self, input: &mut crate::puzzle::InputBuffer, _: &chrono::DateTime<chrono::Utc>) -> crate::puzzle::DeserializationResult<'a> {
    crate::puzzle::check_length(input, SERIALIZATION_LENGTH)?;
    Ok(Box::new(Holiday { calendar: self.calendar.clone(), last: rmp::decode::read_bool(input)? }) as Box<dyn crate::puzzle::PuzzlePiece>)
  }
}

impl crate::puzzle::PuzzlePiece for Holiday {
  fn accept<'a>(self: &mut Self, _: &puzzleverse_core::PuzzleCommand, _: &crate::puzzle::PieceValue) -> crate::puzzle::OutputEvents {
    vec![]
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
    rmp::encode::write_bool(output, self.last).map_err(rmp::encode::ValueWriteError::InvalidDataWrite)?;
    Ok(())
  }
  fn tick(self: &mut Self, time: &chrono::DateTime<chrono::Utc>) -> crate::puzzle::SimpleOutputEvents {
    let current = self.calendar.is_holiday(time.with_timezone(&chrono::Local));
    if current != self.last {
      self.last = current;
      vec![(puzzleverse_core::PuzzleEvent::Changed, crate::puzzle::PieceValue::Bool(current))]
    } else {
      vec![]
    }
  }
  fn reset(&self) -> crate::puzzle::SimpleOutputEvents {
    vec![]
  }
  fn next(self: &Self) -> Option<chrono::DateTime<chrono::Utc>> {
    Some((chrono::Local::today() + chrono::Duration::days(1)).and_hms(0, 0, 0).with_timezone(&chrono::Utc))
  }
  fn update_check<'a, 's>(self: &'s Self, _: &'a crate::puzzle::ConsequenceValueMatcher) -> Option<crate::puzzle::PuzzleConsequence<'a>> {
    None
  }
  fn walk(self: &mut Self, _: &crate::PlayerKey, _: crate::realm::navigation::PlayerNavigationEvent) -> crate::puzzle::SimpleOutputEvents {
    vec![]
  }
}
