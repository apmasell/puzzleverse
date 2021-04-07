use serde::{Deserialize, Serialize};

/// An asset header for a game asset; this is common across all assets
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Asset {
  /// The type of the asset. This is not an enumeration to allow for new asset types to be added in the future. Program should handle the case of an unknown asset gracefully.
  pub asset_type: String,
  /// The player that uploaded this asset, usually encoded as _player_`@`_server_, but not required.
  pub author: String,
  /// The child assets this asset depends on; this is here so that a server can quickly pull all the appropriate assets and inform clients about which assets the require. This is more important for large aggregate assets (_i.e._, realms) over small distinct ones (_e.g._, meshes) and may be empty.
  pub children: Vec<String>,
  /// The actual asset. The format of this data is arbitrary and only interpretable based on `asset_type`. Usually a common external format (_e.g._, PNG) or a Message Pack-encoded Rust data structure.
  pub data: Vec<u8>,
  /// The licence terms applied to this asset. Currently, all licences provided are compatible, so this is mostly informative unless the asset is going to be exported.
  pub licence: Licence,
  /// A friendly name for the asset. This would be used if the asset is to appear in tool palettes and the like.
  pub name: String,
  /// A list of friendly tags to describe this asset to allow for searching.
  pub tags: Vec<String>,
}

/// The licences associated with an asset
///
/// Players should be reminded of the asset when exporting it. Since the system requires unfettered replication of assets between servers, all licenses must permit copying unmodified. All assets should be available to other world builders, so remixing must be supported by all licences.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Licence {
  CreativeCommons(LicenceUses),
  CreativeCommonsNoDerivatives(LicenceUses),
  CreativeCommonsShareALike(LicenceUses),
  PubDom,
}
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum LicenceUses {
  Commercial,
  NonCommercial,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RealmDescription {
  sheets: Vec<SheetDescription>,
}
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SheetDescription {
  pub width: u32,
  pub length: u32,
  pub top: crate::Point,
  pub contents: Vec<ItemDescription>,
}
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum ItemDescription {
  Spray {},
  Object {},
  Connection {},
}
/// Find all assets referenced by this asset. If the asset is corrupt or unknown, `None` is returned
pub fn extract_children(asset_type: &str, asset_data: &[u8]) -> Option<Vec<String>> {
  match asset_type {
    _ => None,
  }
}
