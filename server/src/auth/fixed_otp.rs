struct FixedOTPs {
  secrets: std::collections::HashMap<String, String>,
}
/// Create a simple OTP store
pub fn new(secrets: std::collections::HashMap<String, String>) -> Result<std::sync::Arc<dyn crate::auth::AuthProvider>, String> {
  Ok(std::sync::Arc::new(FixedOTPs { secrets }))
}
#[async_trait::async_trait]
impl crate::auth::OTPStore for FixedOTPs {
  async fn secret(self: &Self, username: &str) -> Vec<String> {
    self.secrets.get(username).cloned().into_iter().collect()
  }
}
