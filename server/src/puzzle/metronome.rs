const SERIALIZATION_LENGTH: u32 = 1;
struct Metronome {
  frequency: u32,
  next: chrono::DateTime<chrono::Utc>,
}

struct MetronomeAsset {
  frequency: u32,
}

impl crate::puzzle::PuzzleAsset for MetronomeAsset {
  fn create(self: &Self, time: &chrono::DateTime<chrono::Utc>) -> Box<dyn crate::puzzle::PuzzlePiece> {
    Box::new(Metronome { frequency: self.frequency, next: *time + chrono::Duration::seconds(self.frequency.into()) })
      as Box<dyn crate::puzzle::PuzzlePiece>
  }
  fn load<'a>(self: &Self, input: &mut crate::puzzle::InputBuffer, time: &chrono::DateTime<chrono::Utc>) -> crate::puzzle::DeserializationResult<'a> {
    crate::puzzle::check_length(input, SERIALIZATION_LENGTH)?;
    let frequency = rmp::decode::read_u32(input)?;
    Ok(Box::new(Metronome { frequency, next: *time + chrono::Duration::seconds(frequency.into()) }) as Box<dyn crate::puzzle::PuzzlePiece>)
  }
}

impl crate::puzzle::PuzzlePiece for Metronome {
  fn accept(self: &mut Self, name: &puzzleverse_core::PuzzleCommand, value: &crate::puzzle::PieceValue) -> crate::puzzle::OutputEvents {
    if name == &puzzleverse_core::PuzzleCommand::Frequency {
      if let crate::puzzle::PieceValue::Num(freq) = value {
        self.frequency = *freq;
      }
    }
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
    rmp::encode::write_u32(output, self.frequency)
  }
  fn tick(self: &mut Self, time: &chrono::DateTime<chrono::Utc>) -> crate::puzzle::SimpleOutputEvents {
    let emit = *time >= self.next;
    while self.next < *time {
      self.next = self.next + chrono::Duration::seconds(self.frequency.into());
    }
    if emit {
      vec![(puzzleverse_core::PuzzleEvent::Cleared, crate::puzzle::PieceValue::Empty)]
    } else {
      vec![]
    }
  }
  fn next(self: &Self) -> Option<chrono::DateTime<chrono::Utc>> {
    Some(self.next)
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
