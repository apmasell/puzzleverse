struct LdapPassword {
  account_attr: String,
  bind_dn: String,
  bind_pw: String,
  connection: bb8::Pool<LDAPConnectionManager<String>>,
  search_base: String,
}

struct LDAPConnectionManager<T: AsRef<str>>(T);
#[async_trait::async_trait]
impl<T: AsRef<str> + Sized + Sync + Send + 'static> bb8::ManageConnection for LDAPConnectionManager<T> {
  type Connection = ldap3::Ldap;
  type Error = ldap3::LdapError;

  async fn connect(&self) -> Result<Self::Connection, Self::Error> {
    let (connection, ldap) = ldap3::LdapConnAsync::new(self.0.as_ref()).await?;
    ldap3::drive!(connection);
    Ok(ldap)
  }

  async fn is_valid(&self, conn: &mut bb8::PooledConnection<'_, Self>) -> Result<(), Self::Error> {
    conn.extended(ldap3::exop::WhoAmI).await?;
    Ok(())
  }

  fn has_broken(&self, conn: &mut Self::Connection) -> bool {
    conn.is_closed()
  }
}
/// Access a Myst Online: Uru Live database for accounts
pub async fn new(
  server_url: String,
  bind_dn: String,
  bind_pw: String,
  search_base: String,
  account_attr: String,
) -> Result<std::sync::Arc<dyn crate::auth::AuthProvider>, String> {
  Ok(std::sync::Arc::new(LdapPassword {
    connection: bb8::Pool::builder()
      .build(LDAPConnectionManager(server_url))
      .await
      .map_err(|e| format!("Failed to create LDAP connection: {:?}", e))?,
    bind_dn,
    bind_pw,
    search_base,
    account_attr,
  }))
}
impl LdapPassword {
  async fn query(self: &Self, username: &str, password: &str) -> Result<bool, String> {
    let mut connection = self.connection.get().await.map_err(|e| e.to_string())?;
    connection.simple_bind(&self.bind_dn, &self.bind_pw).await.map_err(|e| e.to_string())?.success().map_err(|e| e.to_string())?;
    let ldap3::SearchResult(mut results, _) = connection
      .search(&self.search_base, ldap3::Scope::Subtree, &format!("{}={}", &self.account_attr, ldap3::ldap_escape(username)), vec!["cn", "dn"])
      .await
      .map_err(|e| e.to_string())?;
    if results.len() == 1 {
      let dn = ldap3::SearchEntry::construct(results.remove(0)).dn;
      connection.unbind().await.map_err(|e| e.to_string())?;
      Ok(connection.simple_bind(&dn, password).await.map_err(|e| e.to_string())?.success().is_ok())
    } else {
      Ok(false)
    }
  }
}
#[async_trait::async_trait]
impl crate::auth::Password for LdapPassword {
  async fn check(self: &Self, username: &str, password: &str) -> bool {
    match self.query(username, password).await {
      Ok(value) => value,
      Err(e) => {
        eprintln!("Failed to check LDAP password for {}: {}", username, e);
        false
      }
    }
  }
}
