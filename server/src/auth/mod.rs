mod db_otp;
mod fixed_otp;
mod fixed_password;

/// The result of an attempt at authentication
pub enum AuthResult {
  /// The user should be denied access
  Failure,
  /// The user should be granted access by sending a JWT as a response
  SendToken(String),
  /// Send an arbitrary HTTP response to the client
  Page(Result<http::Response<hyper::Body>, http::Error>),
  /// The URL requested is not handled by this authentication provider
  NotHandled,
}

/// A pluggable authentication mechanism
#[async_trait::async_trait]
pub trait AuthProvider: Send + Sync {
  /// Describe the authentication scheme to the client
  fn scheme(self: &Self) -> puzzleverse_core::AuthScheme;
  /// Handle incoming HTTP requests that might be part of authentication
  async fn handle(self: &Self, req: http::Request<hyper::Body>) -> AuthResult;
}

#[derive(serde::Serialize, serde::Deserialize)]
pub enum AuthConfiguration {
  Passwords { users: std::collections::HashMap<String, String> },
  OTPs { users: std::collections::HashMap<String, String> },
  DatabaseOTPs { connection: String },
}
impl AuthConfiguration {
  /// Parse the configuration string provided to the server into an authentication provider, if possible
  pub fn load(&self) -> Result<std::sync::Arc<dyn AuthProvider>, String> {
    match self {
      AuthConfiguration::Passwords { users } => crate::auth::fixed_password::new(&users),
      AuthConfiguration::OTPs { users } => crate::auth::fixed_otp::new(&users),
      AuthConfiguration::DatabaseOTPs { connection } => crate::auth::db_otp::new(&connection),
    }
  }
}

/// Create an authentication provider that deals with unencrypted usernames and passwords
#[async_trait::async_trait]
pub trait Password: Send + Sync {
  /// Check if the username and password provided are valid
  async fn check(self: &Self, username: &str, password: &str) -> bool;
}

#[async_trait::async_trait]
impl<T> AuthProvider for T
where
  T: Password,
{
  fn scheme(self: &Self) -> puzzleverse_core::AuthScheme {
    puzzleverse_core::AuthScheme::Password
  }
  async fn handle(self: &Self, req: http::Request<hyper::Body>) -> AuthResult {
    use bytes::Buf;
    match (req.method(), req.uri().path()) {
      (&http::Method::POST, "/api/auth/password") => match hyper::body::aggregate(req).await {
        Err(e) => {
          eprintln!("Failed to aggregate body: {}", e);
          AuthResult::Failure
        }
        Ok(whole_body) => match serde_json::from_reader::<_, puzzleverse_core::PasswordRequest<String>>(whole_body.reader()) {
          Err(e) => AuthResult::Page(http::Response::builder().status(http::StatusCode::BAD_REQUEST).body(e.to_string().into())),
          Ok(data) => {
            if self.check(&data.username, &data.password).await {
              AuthResult::SendToken(data.username)
            } else {
              AuthResult::Page(http::Response::builder().status(http::StatusCode::UNAUTHORIZED).body("Invalid username or password".into()))
            }
          }
        },
      },
      _ => AuthResult::NotHandled,
    }
  }
}
/// Create an authentication provider that uses one-time passwords as authentication
#[async_trait::async_trait]
pub trait OTPStore: Send + Sync {
  /// Get the secrets for a user
  async fn secret(self: &Self, username: &str) -> Vec<String>;
}

#[async_trait::async_trait]
impl<T> Password for T
where
  T: OTPStore,
{
  async fn check(self: &Self, username: &str, password: &str) -> bool {
    match password.parse::<u32>() {
      Ok(code) => self.secret(&username).await.drain(..).any(|secret| {
        otpauth::TOTP::new(secret).verify(code, 30, std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs())
      }),
      _ => false,
    }
  }
}
