trait Gradiate: Clone {
  fn mix(gradients: impl IntoIterator<Item = (f64, Self)>) -> Self;
}

enum Current<T: Gradiate> {
  Fixed(T),
  BoolControlled { when_true: T, when_false: T, value: std::sync::Arc<std::sync::atomic::AtomicBool>, last_value: bool },
  NumControlled { default_value: T, values: Vec<T>, value: std::sync::Arc<std::sync::atomic::AtomicU32>, last_value: u32 },
}

enum Decay {
  Constant,
  Euclidean,
  Inverted(Box<Decay>),
  Linear,
  Shell,
  Step(f64, Box<Decay>),
}

struct Gradiator<T: Gradiate> {
  sources: Vec<Source<T>>,
  cache: std::collections::HashMap<(u32, u32, u32), T>,
}
struct Source<T: Gradiate> {
  source: Current<T>,
  function: Function,
}
enum Function {
  PointSource { x: u32, y: u32, z: u32, decay: Decay },
}
impl<T: Gradiate> Current<T> {
  fn check_dirty(&mut self) -> bool {
    match self {
      Current::Fixed(_) => false,
      Current::BoolControlled { value, last_value, .. } => {
        let current = value.load(std::sync::atomic::Ordering::Relaxed);
        if &current == last_value {
          false
        } else {
          *last_value = current;
          true
        }
      }
      Current::NumControlled { value, last_value, .. } => {
        let current = value.load(std::sync::atomic::Ordering::Relaxed);
        if &current == last_value {
          false
        } else {
          *last_value = current;
          true
        }
      }
    }
  }
  fn value(&self) -> &T {
    match self {
      Current::Fixed(v) => v,
      Current::BoolControlled { last_value, when_true, when_false, .. } => {
        if *last_value {
          when_true
        } else {
          when_false
        }
      }
      Current::NumControlled { last_value, values, default_value, .. } => values.get(*last_value as usize).unwrap_or(default_value),
    }
  }
}
impl Decay {
  fn compute(&self, delta_x: f64, delta_y: f64, delta_z: f64) -> f64 {
    match self {
      Decay::Constant => 1.0,
      Decay::Euclidean => (delta_x * delta_x + delta_y * delta_y + delta_z * delta_z).sqrt(),
      Decay::Inverted(original) => 1.0 - original.compute(delta_x, delta_y, delta_z),
      Decay::Linear => (delta_x * delta_x + delta_y * delta_y + delta_z * delta_z),
      Decay::Shell => (delta_x * delta_x + delta_y * delta_y + delta_z * delta_z).log(1.0 / 3.0),
      Decay::Step(cutoff, original) => {
        let v = original.compute(delta_x, delta_y, delta_z);
        if v < *cutoff {
          0.0
        } else {
          v
        }
      }
    }
  }
}
impl<T: Gradiate> Gradiator<T> {
  fn get(&mut self, x: u32, y: u32, z: u32) -> T {
    let mut clear = false;
    for source in self.sources.iter_mut() {
      if source.source.check_dirty() {
        clear = true;
      }
    }
    if clear {
      self.cache.clear();
    }
    match self.cache.entry((x, y, z)) {
      std::collections::hash_map::Entry::Vacant(v) => {
        v.insert(T::mix(self.sources.iter().map(|s| (s.function.distance(x, y, z), s.source.value().clone())))).clone()
      }
      std::collections::hash_map::Entry::Occupied(o) => o.get().clone(),
    }
  }
}
impl Function {
  fn distance(&self, ix: u32, iy: u32, iz: u32) -> f64 {
    match self {
      Function::PointSource { x, y, z, decay } => {
        decay.compute(abs_difference(*x, ix) as f64, abs_difference(*y, iy) as f64, abs_difference(*z, iz) as f64)
      }
    }
  }
}

fn abs_difference<T: std::ops::Sub<Output = T> + Ord>(x: T, y: T) -> T {
  if x < y {
    y - x
  } else {
    x - y
  }
}

impl Gradiate for f64 {
  fn mix(gradients: impl IntoIterator<Item = (f64, Self)>) -> Self {
    let mut value = 0.0;
    let mut count = 0;
    for (weight, item) in gradients {
      value += weight * item;
      count += 1;
    }
    if count > 0 {
      value / count as f64
    } else {
      0.0
    }
  }
}
impl Gradiate for bevy::render::color::Color {
  fn mix(gradients: impl IntoIterator<Item = (f64, Self)>) -> Self {
    // Blending from http://jcgt.org/published/0002/02/09/ and clarified in https://computergraphics.stackexchange.com/questions/4651/additive-blending-with-weighted-blended-order-independent-transparency/5937
    let mut r_value = 0.0;
    let mut g_value = 0.0;
    let mut b_value = 0.0;
    let mut a_value = 0.0;
    let mut reveal = 1.0;
    for (weight, item) in gradients {
      let weight = weight as f32;
      let [r, g, b, a] = item.as_linear_rgba_f32();
      r_value += weight * a * r;
      g_value += weight * a * g;
      b_value += weight * a * b;
      a_value += weight * a;
      reveal *= 1.0 - a;
    }
    a_value = a_value.max(1e-5);
    bevy::render::color::Color::RgbaLinear { red: r_value / a_value, green: g_value / a_value, blue: b_value / a_value, alpha: 1.0 - reveal }
  }
}
