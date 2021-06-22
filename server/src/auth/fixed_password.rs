struct FixedPasswords {
  secrets: std::collections::HashMap<String, String>,
}
#[async_trait::async_trait]
impl crate::auth::Password for FixedPasswords {
  async fn check(self: &Self, username: &str, password: &str) -> bool {
    self.secrets.get(username).map(|p| p == password).unwrap_or(false)
  }
}
/// Create a simple password store
pub fn new(secrets: std::collections::HashMap<String, String>) -> Result<std::sync::Arc<dyn crate::auth::AuthProvider>, String> {
  Ok(std::sync::Arc::new(FixedPasswords { secrets }))
}
