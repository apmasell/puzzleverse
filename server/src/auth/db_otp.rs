use crate::diesel::prelude::*;
struct DatabaseOTPs {
  connection: r2d2::Pool<diesel::r2d2::ConnectionManager<diesel::pg::PgConnection>>,
}
/// Create a simple OTP store
pub fn new(database_url: &str) -> Result<std::sync::Arc<dyn crate::auth::AuthProvider>, String> {
  let manager = diesel::r2d2::ConnectionManager::<diesel::pg::PgConnection>::new(database_url);
  Ok(std::sync::Arc::new(DatabaseOTPs {
    connection: r2d2::Pool::builder().build(manager).map_err(|e| format!("Failed to create OTP database connection: {}", e))?,
  }))
}
#[async_trait::async_trait]
impl crate::auth::OTPStore for DatabaseOTPs {
  async fn secret(self: &Self, username: &str) -> Vec<String> {
    use crate::schema::authotp::dsl as auth_otp_schema;
    match self.connection.get() {
      Ok(db_connection) => {
        match auth_otp_schema::authotp.select(auth_otp_schema::code).filter(auth_otp_schema::name.eq(username)).get_results(&db_connection) {
          Ok(results) => results,
          Err(e) => {
            eprintln!("Failed to fetch OTPs for {}: {}", username, e);
            vec![]
          }
        }
      }
      Err(e) => {
        eprintln!("Failed to get connection to fetch OTPs for {}: {}", username, e);
        vec![]
      }
    }
  }
}
