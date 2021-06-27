use crate::diesel::prelude::*;
struct UruDatabase {
  connection: r2d2::Pool<diesel::r2d2::ConnectionManager<diesel::pg::PgConnection>>,
}
/// Access a Myst Online: Uru Live database for accounts
pub fn new(database_url: String) -> Result<std::sync::Arc<dyn crate::auth::AuthProvider>, String> {
  let manager = diesel::r2d2::ConnectionManager::<diesel::pg::PgConnection>::new(database_url);
  Ok(std::sync::Arc::new(UruDatabase {
    connection: r2d2::Pool::builder().build(manager).map_err(|e| format!("Failed to create Uru database connection: {}", e))?,
  }))
}
#[async_trait::async_trait]
impl crate::auth::Password for UruDatabase {
  async fn check(self: &Self, username: &str, password: &str) -> bool {
    match self.connection.get() {
      Ok(db_connection) => {
        #[derive(diesel::QueryableByName, PartialEq, Debug)]
        struct UruPassword {
          #[sql_type = "diesel::sql_types::Text"]
          pub pass_hash: String,
        }
        match diesel::sql_query("SELECT \"PassHash\" as pass_hash FROM \"Accounts\" WHERE \"Login\" = $1")
          .bind::<diesel::sql_types::Text, _>(username)
          .load::<UruPassword>(&db_connection)
        {
          Ok(results) => {
            let mut digest = sha1::Sha1::new();
            digest.update(password.as_bytes());
            let hash = digest.hexdigest();
            results.iter().any(|h| h.pass_hash == hash)
          }
          Err(diesel::result::Error::NotFound) => false,
          Err(e) => {
            eprintln!("Failed to fetch Uru for {}: {}", username, e);
            false
          }
        }
      }
      Err(e) => {
        eprintln!("Failed to get connection to fetch Uru for {}: {}", username, e);
        false
      }
    }
  }
}
