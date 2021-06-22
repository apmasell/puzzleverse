use crate::diesel::prelude::*;
struct PhpBBDb<T, B>
where
  T: 'static + diesel::Connection<Backend = B, TransactionManager = diesel::connection::AnsiTransactionManager>,
  B: 'static + diesel::backend::Backend<RawValue = [u8]> + diesel::backend::UsesAnsiSavepointSyntax,
  B::QueryBuilder: Default + diesel::query_builder::QueryBuilder<B>,
{
  connection: r2d2::Pool<diesel::r2d2::ConnectionManager<T>>,
}
/// Create a phpBB-backed store
pub fn new<T, B>(database_url: String) -> Result<std::sync::Arc<dyn crate::auth::AuthProvider>, String>
where
  T: 'static + diesel::Connection<Backend = B, TransactionManager = diesel::connection::AnsiTransactionManager>,
  B: 'static + diesel::backend::Backend<RawValue = [u8]> + diesel::backend::UsesAnsiSavepointSyntax,
  B::QueryBuilder: Default + diesel::query_builder::QueryBuilder<B>,
{
  let manager = diesel::r2d2::ConnectionManager::<T>::new(database_url);
  Ok(std::sync::Arc::new(PhpBBDb {
    connection: r2d2::Pool::builder().build(manager).map_err(|e| format!("Failed to create phpBB database connection: {}", e))?,
  }))
}
#[async_trait::async_trait]
impl<T, B> crate::auth::Password for PhpBBDb<T, B>
where
  T: 'static + diesel::Connection<Backend = B, TransactionManager = diesel::connection::AnsiTransactionManager>,
  B: 'static + diesel::backend::Backend<RawValue = [u8]> + diesel::backend::UsesAnsiSavepointSyntax,
  B::QueryBuilder: Default + diesel::query_builder::QueryBuilder<B>,
{
  async fn check(self: &Self, username: &str, password: &str) -> bool {
    match self.connection.get() {
      Ok(db_connection) => {
        use crate::diesel::query_builder::QueryBuilder;
        #[derive(diesel::QueryableByName, PartialEq, Debug)]
        struct PhpBBPassword {
          #[sql_type = "diesel::sql_types::Text"]
          pub user_password: String,
        }
        let mut query = B::QueryBuilder::default();
        query.push_sql("SELECT user_password FROM phpbb_users WHERE username_clean = ");
        query.push_bind_param();
        query.push_sql(" AND user_type IN (0, 3)");
        match diesel::sql_query(query.finish()).bind::<diesel::sql_types::Text, _>(username).load::<PhpBBPassword>(&db_connection) {
          Ok(results) => results.iter().any(|h| phpbb_pwhash::check_hash(&h.user_password, password) == phpbb_pwhash::CheckHashResult::Valid),
          Err(diesel::result::Error::NotFound) => false,
          Err(e) => {
            eprintln!("Failed to check phpBB password for {}: {}", username, e);
            false
          }
        }
      }
      Err(e) => {
        eprintln!("Failed to get connection to check phpBB password for {}: {}", username, e);
        false
      }
    }
  }
}
