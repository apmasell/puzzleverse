table! {
    authotp (name, code) {
        name -> Text,
        code -> Text,
    }
}

table! {
    bookmark (player, asset) {
        player -> Int4,
        asset -> Text,
        kind -> Varchar,
    }
}

table! {
    localplayerchat (sender, recipient, created) {
        sender -> Int4,
        recipient -> Int4,
        created -> Timestamptz,
        body -> Text,
    }
}

table! {
    player (id) {
        id -> Int4,
        name -> Text,
        realm -> Nullable<Int4>,
        debuted -> Bool,
        message_acl -> Bytea,
        online_acl -> Bytea,
        location_acl -> Bytea,
        new_realm_access_acl -> Bytea,
        new_realm_admin_acl -> Bytea,
        last_login -> Timestamptz,
        created -> Timestamptz,
    }
}

table! {
    publickey (player, name) {
        player -> Int4,
        name -> Text,
        public_key -> Bytea,
    }
}

table! {
    realm (id) {
        id -> Int4,
        principal -> Text,
        name -> Text,
        owner -> Int4,
        asset -> Text,
        state -> Bytea,
        settings -> Bytea,
        access_acl -> Bytea,
        admin_acl -> Bytea,
        in_directory -> Bool,
        initialised -> Bool,
        seed -> Int4,
        created -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

table! {
    realmchat (principal, created, realm) {
        realm -> Int4,
        principal -> Text,
        created -> Timestamptz,
        body -> Text,
    }
}

table! {
    remoteplayerchat (remote_player, remote_server, created, player, state) {
        player -> Int4,
        state -> Varchar,
        remote_player -> Text,
        remote_server -> Text,
        created -> Timestamptz,
        body -> Text,
    }
}

table! {
    serveracl (category) {
        category -> Varchar,
        acl -> Bytea,
    }
}

joinable!(bookmark -> player (player));
joinable!(publickey -> player (player));
joinable!(realmchat -> realm (realm));
joinable!(remoteplayerchat -> player (player));

allow_tables_to_appear_in_same_query!(authotp, bookmark, localplayerchat, player, publickey, realm, realmchat, remoteplayerchat, serveracl,);
