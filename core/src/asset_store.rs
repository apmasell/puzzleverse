/// A place where assets can be stored for later access
pub trait AssetStore: Send + Sync {
  /// Determine if the asset ID provided is available
  fn check(&self, asset: &str) -> bool;
  /// Retrieve an asset from the store
  fn pull(&self, asset: &str) -> LoadResult;
  /// Store a new asset in the store
  fn push(&self, asset: &str, value: &crate::asset::Asset);
}

#[async_trait::async_trait]
pub trait AsyncAssetStore: Send + Sync {
  /// Determine if the asset ID provided is available
  async fn check(&self, asset: &str) -> bool;
  async fn missing(&self, asset: &str) -> bool {
    !self.check(asset).await
  }
  /// Retrieve an asset from the store
  async fn pull(&self, asset: &str) -> LoadResult;
  /// Store a new asset in the store
  async fn push(&self, asset: &str, value: &crate::asset::Asset);
}

/// The type of result when attempting to pull an asset from the store
pub enum LoadResult {
  /// The asset was found, but cannot be decoded
  Corrupt,
  /// Some other error occurred access the asset
  InternalError,
  /// The asset was loaded successfully
  Loaded(crate::asset::Asset),
  /// The asset was not found in the asset store
  Unknown,
}

/// An asset store backed by a directory on the file system
pub struct FileSystemStore<T: AsRef<std::path::Path> + Send + Sync> {
  root: T,
  splits: Vec<usize>,
}

impl<T: AsRef<std::path::Path> + Send + Sync> FileSystemStore<T> {
  /// Create a new file store backed by a directory
  /// * `directory` - the directory root holding the asset files
  /// * `splits` - since most file systems would get angry with a directory containing many files, this creates hierarchical directory structure by breaking up an asset ID. A split of 4, 2, will transform `AAAABBCCCC` into `AAAA/BB/CCCC`
  pub fn new(directory: T, splits: impl IntoIterator<Item = usize>) -> FileSystemStore<T> {
    FileSystemStore { root: directory, splits: splits.into_iter().collect() }
  }
  fn get_path(&self, asset: &str) -> std::path::PathBuf {
    let mut result = self.root.as_ref().to_path_buf();
    result.extend(self.splits.iter().scan(0 as usize, |s, &l| match asset.get(*s..(*s + l)) {
      Some(output) => {
        *s += l;
        if output.is_empty() {
          None
        } else {
          Some(output.to_string())
        }
      }
      None => None,
    }));
    result.push(asset);
    result
  }
}
impl<T: AsRef<std::path::Path> + Send + Sync> AssetStore for FileSystemStore<T> {
  fn check(&self, asset: &str) -> bool {
    std::fs::metadata(&self.get_path(asset)).is_ok()
  }

  fn pull(&self, asset: &str) -> LoadResult {
    match std::fs::File::open(&self.get_path(asset)) {
      Ok(b) => match rmp_serde::from_read::<_, crate::asset::Asset>(b) {
        Ok(result) => LoadResult::Loaded(result),
        Err(e) => {
          eprintln!("Asset {} is corrupt: {}", asset, e);
          LoadResult::Corrupt
        }
      },
      Err(e) => {
        if e.kind() == std::io::ErrorKind::NotFound {
          LoadResult::Unknown
        } else {
          eprintln!("Failed to get asset {}: {}", asset, e);
          LoadResult::InternalError
        }
      }
    }
  }

  fn push(&self, asset: &str, value: &crate::asset::Asset) {
    let path = self.get_path(asset);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    rmp_serde::encode::write(&mut std::fs::OpenOptions::new().write(true).open(&path).unwrap(), value).unwrap();
  }
}

#[async_trait::async_trait]
impl<T: AssetStore> AsyncAssetStore for T {
  async fn check(&self, asset: &str) -> bool {
    <T as AssetStore>::check(self, asset)
  }

  async fn pull(&self, asset: &str) -> LoadResult {
    <T as AssetStore>::pull(self, asset)
  }

  async fn push(&self, asset: &str, value: &crate::asset::Asset) {
    <T as AssetStore>::push(self, asset, value)
  }
}
