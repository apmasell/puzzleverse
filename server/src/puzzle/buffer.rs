use chrono::{DateTime, Utc};
use rand::seq::SliceRandom;

const SERIALIZATION_LENGTH: u32 = 2;

struct Buffer<T: Sync> {
  contents: std::collections::vec_deque::VecDeque<T>,
  capacity: usize,
}

enum BufferType {
  Bool,
  Int,
  Realm,
}
struct BufferAsset {
  length: u32,
  buffer_type: BufferType,
}

impl crate::puzzle::PuzzleAsset for BufferAsset {
  fn create(self: &Self, _: &chrono::DateTime<chrono::Utc>) -> Box<dyn crate::puzzle::PuzzlePiece> {
    let length = self.length as usize;
    match self.buffer_type {
      BufferType::Bool => {
        std::boxed::Box::new(Buffer::<bool> { contents: std::collections::vec_deque::VecDeque::with_capacity(length), capacity: length })
          as Box<dyn crate::puzzle::PuzzlePiece>
      }
      BufferType::Int => {
        std::boxed::Box::new(Buffer::<u32> { contents: std::collections::vec_deque::VecDeque::with_capacity(length), capacity: length })
          as Box<dyn crate::puzzle::PuzzlePiece>
      }
      BufferType::Realm => std::boxed::Box::new(Buffer::<crate::puzzle::RealmLink> {
        contents: std::collections::vec_deque::VecDeque::with_capacity(length),
        capacity: length,
      }) as Box<dyn crate::puzzle::PuzzlePiece>,
    }
  }
  fn load<'a>(self: &Self, input: &mut crate::puzzle::InputBuffer, _: &chrono::DateTime<chrono::Utc>) -> crate::puzzle::DeserializationResult<'a> {
    match self.buffer_type {
      BufferType::Bool => load::<bool, crate::puzzle::InputBuffer>(input),
      BufferType::Int => load::<u32, crate::puzzle::InputBuffer>(input),
      BufferType::Realm => load::<crate::puzzle::RealmLink, crate::puzzle::InputBuffer>(input),
    }
  }
}

fn load<'a, T, R>(input: &mut crate::puzzle::InputBuffer) -> crate::puzzle::DeserializationResult<'a>
where
  T: 'a,
  T: Clone,
  T: Send + Sync,
  Vec<T>: Into<crate::puzzle::PieceValue>,
  T: Into<crate::puzzle::PieceValue>,
  T: crate::puzzle::EncodeValue,
  T: crate::puzzle::DecodeSaved,
  T: crate::puzzle::ExtractValue,
{
  crate::puzzle::check_length(input, SERIALIZATION_LENGTH)?;
  let length = rmp::decode::read_u32(input)? as usize;
  let mut buffer = Buffer::<T> { contents: std::collections::vec_deque::VecDeque::new(), capacity: length };
  for _ in 0..rmp::decode::read_array_len(input)? {
    buffer.contents.push_back(T::read(input)?);
  }
  Ok(Box::new(buffer) as Box<dyn crate::puzzle::PuzzlePiece>)
}

impl<'x, T> crate::puzzle::PuzzlePiece for Buffer<T>
where
  T: Clone,
  T: Send + Sync,
  Vec<T>: Into<crate::puzzle::PieceValue>,
  T: Into<crate::puzzle::PieceValue>,
  T: crate::puzzle::EncodeValue,
  T: crate::puzzle::ExtractValue,
{
  fn accept<'a>(self: &mut Self, name: &puzzleverse_core::PuzzleCommand, value: &crate::puzzle::PieceValue) -> crate::puzzle::OutputEvents {
    if name == &puzzleverse_core::PuzzleCommand::Insert {
      T::extract_value(value)
        .map(|v| {
          self.contents.push_back(v.clone());
          while self.contents.len() > self.capacity {
            self.contents.pop_front();
          }
          vec![
            crate::puzzle::OutputEvent::Event(puzzleverse_core::PuzzleEvent::Changed, self.contents.iter().cloned().collect::<Vec<T>>().into()),
            crate::puzzle::OutputEvent::Event(puzzleverse_core::PuzzleEvent::Selected, v.into()),
          ]
        })
        .unwrap_or_else(|| vec![])
    } else if name == &puzzleverse_core::PuzzleCommand::Clear && *value == crate::puzzle::PieceValue::Empty {
      self.contents.clear();
      vec![
        crate::puzzle::OutputEvent::Event(puzzleverse_core::PuzzleEvent::Changed, self.contents.iter().cloned().collect::<Vec<T>>().into()),
        crate::puzzle::OutputEvent::Event(puzzleverse_core::PuzzleEvent::Cleared, crate::puzzle::PieceValue::Empty),
      ]
    } else if name == &puzzleverse_core::PuzzleCommand::Toggle && *value == crate::puzzle::PieceValue::Empty && !self.contents.is_empty() {
      let mut contents: Vec<T> = self.contents.drain(..).collect();
      contents.shuffle(&mut rand::thread_rng());
      self.contents.extend(contents.drain(..));
      vec![crate::puzzle::OutputEvent::Event(puzzleverse_core::PuzzleEvent::Changed, self.contents.iter().cloned().collect::<Vec<T>>().into())]
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
    rmp::encode::write_u32(output, self.contents.capacity() as u32)?;
    rmp::encode::write_array_len(output, self.contents.len() as u32)?;

    for v in self.contents.iter() {
      v.write(output)?;
    }
    Ok(())
  }
  fn tick(self: &mut Self, _: &chrono::DateTime<chrono::Utc>) -> crate::puzzle::SimpleOutputEvents {
    vec![]
  }

  fn next(self: &Self) -> Option<DateTime<Utc>> {
    None
  }
  fn reset(&self) -> crate::puzzle::SimpleOutputEvents {
    vec![]
  }
  fn update_check<'a, 's>(self: &'s Self, state: &'a crate::puzzle::ConsequenceValueMatcher) -> Option<crate::puzzle::PuzzleConsequence<'a>> {
    self.contents.front().map(|f| state.apply(&f.clone().into())).flatten()
  }

  fn walk(self: &mut Self, _: &crate::PlayerKey, _: crate::realm::navigation::PlayerNavigationEvent) -> crate::puzzle::SimpleOutputEvents {
    vec![]
  }
}
