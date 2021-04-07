/// This is the cost to cross a puzzle-gated boundary. Basically, if we can walk this number steps to avoid crossing a gate, we will.
const BOUNDARY_PENALTY: u32 = 100;
/// This is the time in ms it takes to walk one tile
const STEP_TIME: u32 = 500;
/// The cost of a straight step
const STRAIGHT: u32 = 10;
/// The cost of a diagonal (roughly 10*sqrt(2))
const DIAGONAL: u32 = 14;
// The time in ms it takes to warp in/out
pub(crate) const WARP_TIME: u32 = 800;

/// A connection between two grids where players can navigate
pub struct Bridge {
  edge: PlatformEdge,
  location: std::ops::Range<u32>,
  target_platform: usize,
  condition: BridgeCondition,
}

/// The rule to deicde the transition between two grids
pub enum BridgeCondition {
  /// The player can always walk with the animation and time penalty given
  Static(puzzleverse_core::CharacterAnimation, u32),
  /// The player can only walk when the puzzle has permitted it. If they can, they use the animation and time penalty given
  PuzzleGated(std::sync::Arc<std::sync::atomic::AtomicBool>, puzzleverse_core::CharacterAnimation, u32),
}

/// What player interaction is available at a location on the ground
pub struct InteractionInformation {
  /// The piece ID
  piece: usize,
  /// The animation that should be shown when interaction is occuring
  animation: puzzleverse_core::CharacterAnimation,
  /// The duration of the animation
  duration: u32,
}

/// The spaces where a player could move
pub enum Ground {
  Obstacle,
  GatedObstacle(std::sync::Arc<std::sync::atomic::AtomicBool>),
  Pieces { interaction: Option<InteractionInformation>, proximity: Vec<usize> },
  Empty,
}

/// A grid of possible player locations
pub struct Platform {
  width: u32,
  length: u32,
  terrain: std::collections::BTreeMap<(u32, u32), Ground>,
  animation: puzzleverse_core::CharacterAnimation,
  bridges: Vec<Bridge>,
}

#[derive(Debug, PartialEq)]
pub enum PlatformEdge {
  Top,
  Left,
  Right,
  Bottom,
}

#[derive(Clone)]
pub enum PlayerNavigationEvent {
  Enter,
  Leave,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpawnArea {
  platform: usize,
  x1: u32,
  y1: u32,
  x2: u32,
  y2: u32,
}

pub struct RealmManifold {
  platforms: Vec<Platform>,
  default_spawn: SpawnArea,
  spawn_points: std::collections::HashMap<String, SpawnArea>,
}

impl RealmManifold {
  /// Determine if there are any puzzle pieces buried in at a particular location that the player can interact with
  pub fn active_proximity(&self, position: &puzzleverse_core::Point) -> impl std::iter::Iterator<Item = usize> + '_ {
    (match self.platforms.get(position.platform as usize).map(|platform| platform.terrain.get(&(position.x, position.y))).flatten() {
      Some(Ground::Pieces { proximity, .. }) => Some(proximity.iter().copied()),
      _ => None,
    })
    .into_iter()
    .flatten()
  }
  /// Determine the animation that should be used as a player navigates between two points
  pub fn animation(
    &self,
    source: &puzzleverse_core::Point,
    target: &puzzleverse_core::Point,
    is_first: bool,
  ) -> (&puzzleverse_core::CharacterAnimation, chrono::Duration, bool) {
    if source.platform == target.platform {
      (
        self.platforms.get(target.platform as usize).map(|s| &s.animation).unwrap_or(&puzzleverse_core::CharacterAnimation::Walk),
        chrono::Duration::milliseconds(STEP_TIME.into()),
        false,
      )
    } else {
      self
        .platforms
        .get(source.platform as usize)
        .map(|s| {
          s.bridges
            .iter()
            .filter(|ts| ts.target_platform == target.platform as usize)
            .map(|ts| match &ts.condition {
              BridgeCondition::Static(animation, time) => (animation, chrono::Duration::milliseconds(*time as i64), false),
              BridgeCondition::PuzzleGated(gate, animation, time) => {
                if is_first && gate.load(std::sync::atomic::Ordering::Relaxed) {
                  (animation, chrono::Duration::milliseconds(*time as i64), false)
                } else {
                  (&puzzleverse_core::CharacterAnimation::Confused, chrono::Duration::milliseconds(700), true)
                }
              }
            })
            .nth(0)
        })
        .flatten()
        .unwrap_or((&puzzleverse_core::CharacterAnimation::Walk, chrono::Duration::milliseconds(STEP_TIME.into()), false))
    }
  }
  pub fn find_adjacent_or_same(&self, target: &puzzleverse_core::Point) -> puzzleverse_core::Point {
    if let Some(platform) = self.platforms.get(target.platform as usize) {
      let mut possibilites = Vec::new();
      if target.x > 0 {
        possibilites.push(puzzleverse_core::Point { x: target.x - 1, y: target.y, platform: target.platform });
        if target.y > 0 {
          possibilites.push(puzzleverse_core::Point { x: target.x - 1, y: target.y - 1, platform: target.platform });
        }
        if target.y < platform.length - 1 {
          possibilites.push(puzzleverse_core::Point { x: target.x - 1, y: target.y + 1, platform: target.platform });
        }
      }

      if target.x < platform.width - 1 {
        possibilites.push(puzzleverse_core::Point { x: target.x + 1, y: target.y, platform: target.platform });
        if target.y > 0 {
          possibilites.push(puzzleverse_core::Point { x: target.x + 1, y: target.y - 1, platform: target.platform });
        }
        if target.y < platform.length - 1 {
          possibilites.push(puzzleverse_core::Point { x: target.x + 1, y: target.y + 1, platform: target.platform });
        }
      }
      if target.y > 0 {
        possibilites.push(puzzleverse_core::Point { x: target.x, y: target.y - 1, platform: target.platform });
      }
      if target.y < platform.length - 1 {
        possibilites.push(puzzleverse_core::Point { x: target.x, y: target.y + 1, platform: target.platform });
      }
      possibilites.retain(|p| {
        platform
          .terrain
          .get(&(p.x, p.y))
          .map(|g| match g {
            Ground::Obstacle => false,
            Ground::GatedObstacle(gate) => gate.load(std::sync::atomic::Ordering::Relaxed),
            Ground::Pieces { .. } => true,
            Ground::Empty => true,
          })
          .unwrap_or(true)
      });
      use rand::seq::SliceRandom;
      possibilites.shuffle(&mut rand::thread_rng());
      possibilites.into_iter().next().unwrap_or(target.clone())
    } else {
      target.clone()
    }
  }
  /// Determine what animation should be used to indicate a player is interacting with an item at this location
  pub fn interaction_animation(&self, target: &puzzleverse_core::Point) -> Option<(&puzzleverse_core::CharacterAnimation, chrono::Duration)> {
    self
      .platforms
      .get(target.platform as usize)
      .map(|s| s.terrain.get(&(target.x, target.y)))
      .flatten()
      .map(|ground| match ground {
        Ground::Pieces { interaction, .. } => {
          interaction.as_ref().map(|info| (&info.animation, chrono::Duration::milliseconds(info.duration.into())))
        }
        _ => None,
      })
      .flatten()
  }
  /// Determine if there is a puzzle piece to interact with at the specified position
  pub fn interaction_target(&self, target: &puzzleverse_core::Point) -> Option<usize> {
    self
      .platforms
      .get(target.platform as usize)
      .map(|s| s.terrain.get(&(target.x, target.y)))
      .flatten()
      .map(|ground| match ground {
        Ground::Pieces { interaction, .. } => interaction.as_ref().map(|info| info.piece),
        _ => None,
      })
      .flatten()
  }
  /// Determine if a player can occupy this position
  pub fn verify(&self, position: &puzzleverse_core::Point) -> bool {
    self
      .platforms
      .get(position.platform as usize)
      .map(|platform| {
        position.y < platform.length
          && position.x < platform.width
          && match platform.terrain.get(&(position.x, position.y)) {
            Some(Ground::Obstacle) => false,
            Some(Ground::GatedObstacle(gate)) => gate.load(std::sync::atomic::Ordering::Relaxed),
            Some(_) => true,
            None => false,
          }
      })
      .unwrap_or(false)
  }
  /// Determine if a player can cross between two grids in the current state
  pub fn verify_join(&self, start: &puzzleverse_core::Point, end: &puzzleverse_core::Point, skip_gate_check: bool) -> bool {
    if start.platform == end.platform {
      true
    } else {
      self
        .platforms
        .get(start.platform as usize)
        .map(|start_platform| {
          start_platform.bridges.iter().any(|bridge| {
            bridge.target_platform == end.platform as usize
              && match &bridge.edge {
                PlatformEdge::Top => start.y == 0 && bridge.location.contains(&start.x),
                PlatformEdge::Left => start.x == 0 && bridge.location.contains(&start.y),
                PlatformEdge::Right => start.x == start_platform.width - 1 && bridge.location.contains(&start.y),
                PlatformEdge::Bottom => start.y == start_platform.length - 1 && bridge.location.contains(&start.x),
              }
              && (skip_gate_check
                || match &bridge.condition {
                  BridgeCondition::Static(_, _) => true,
                  BridgeCondition::PuzzleGated(gate, _, _) => gate.load(std::sync::atomic::Ordering::Relaxed),
                })
          })
        })
        .unwrap_or(false)
    }
  }
  /// Determine a point at which a player will materialise for a given spawn point
  pub fn warp(&mut self, name: Option<&str>) -> Option<puzzleverse_core::Point> {
    let point = match name {
      Some(n) => self.spawn_points.get(n)?,
      None => &self.default_spawn,
    };
    match self.platforms.get_mut(point.platform) {
      Some(platform) => {
        use rand::seq::SliceRandom;
        let mut targets: Vec<_> = Vec::new();
        for x in point.x1..point.x2 {
          targets.extend(
            (point.y1..point.y2)
              .filter(|&y| match platform.terrain.get(&(x, y)) {
                Some(Ground::Obstacle) => false,
                Some(Ground::GatedObstacle(gate)) => gate.load(std::sync::atomic::Ordering::Relaxed),
                _ => true,
              })
              .map(move |y| (x, y)),
          );
        }

        targets.shuffle(&mut rand::thread_rng());
        targets.get(0).map(|&(x, y)| puzzleverse_core::Point { platform: point.platform as u32, x, y })
      }
      None => None,
    }
  }
}
