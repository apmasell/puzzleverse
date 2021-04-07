CREATE TABLE Player (
    id serial PRIMARY KEY NOT NULL,
    name text NOT NULL,
    realm int4,
    message_acl bytea NOT NULL,
    online_acl bytea NOT NULL,
    location_acl bytea NOT NULL,
    new_realm_access_acl bytea NOT NULL,
    new_realm_admin_acl bytea NOT NULL,
    last_login timestamp WITH time zone NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created timestamp WITH time zone NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE LocalPlayerChat (
    sender int4 NOT NULL,
    recipient int4 NOT NULL,
    created timestamp WITH time zone NOT NULL,
    body text NOT NULL,
    PRIMARY KEY (sender, recipient, created),
    CONSTRAINT local_playerchat_sender_player_id FOREIGN KEY (sender) REFERENCES Player (id),
    CONSTRAINT local_playerchat_recipient_player_id FOREIGN KEY (recipient) REFERENCES Player (id)
);

CREATE INDEX localplayerchat_by_timestamp ON localplayerchat (sender, recipient, created);

CREATE TABLE RemotePlayerChat (
    player int4 NOT NULL,
    state varchar(1) NOT NULL,
    remote_player text NOT NULL,
    remote_server text NOT NULL,
    created timestamp WITH time zone NOT NULL,
    body text NOT NULL,
    PRIMARY KEY (remote_player, remote_server, created, player, state),
    CONSTRAINT realm_playerchat_player_id FOREIGN KEY (player) REFERENCES Player (id)
);

CREATE INDEX remoteplayerchat_by_timestamp ON remoteplayerchat (player, remote_server, remote_player, created);

CREATE TABLE Realm (
    id serial PRIMARY KEY NOT NULL,
    principal text UNIQUE NOT NULL,
    name text NOT NULL,
    owner int4 NOT NULL,
    asset text NOT NULL,
    state bytea NOT NULL,
    settings bytea NOT NULL,
    access_acl bytea NOT NULL,
    admin_acl bytea NOT NULL,
    in_directory boolean NOT NULL,
    initialised boolean NOT NULL,
    seed int4 NOT NULL,
    created timestamp WITH time zone NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at timestamp WITH time zone NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT realm_player_id FOREIGN KEY (owner) REFERENCES Player (id),
    CONSTRAINT realm_only_per_player UNIQUE (owner, asset)
);

SELECT diesel_manage_updated_at('Realm');

ALTER TABLE Player
    ADD CONSTRAINT player_realm_id FOREIGN KEY (realm) REFERENCES Realm (id);

CREATE TABLE RealmChat (
    realm int4 NOT NULL,
    principal text NOT NULL,
    created timestamp WITH time zone NOT NULL,
    body text NOT NULL,
    PRIMARY KEY (principal, created, realm),
    CONSTRAINT realmchat_realm_id FOREIGN KEY (realm) REFERENCES Realm (id)
);

CREATE INDEX realmchat_by_timestamp ON realmchat (realm, created);

CREATE TABLE ServerACL (
    category varchar(1) NOT NULL,
    acl bytea NOT NULL,
    PRIMARY KEY (category)
);

CREATE TABLE AuthOTP (
    name text NOT NULL,
    code text NOT NULL,
    PRIMARY KEY (name, code)
);

CREATE TABLE Bookmark (
	  player int4 NOT NULL,
    asset text NOT NULL,
    kind varchar(1) NOT NULL,
    PRIMARY KEY(player, asset),
    CONSTRAINT bookmark_player_id FOREIGN KEY (player) REFERENCES Player (id)
);

CREATE VIEW LastMessages AS
    SELECT player AS id, MAX(created) AS last_time, remote_player || '@' || remote_server AS principal
      FROM RemotePlayerChat
      WHERE state = 'r'
      GROUP BY player, remote_server, remote_player
    UNION ALL
    SELECT recipient AS id, MAX(created) AS last_time, (SELECT name FROM Player WHERE player.id = sender) AS principal
      FROM LocalPlayerChat
      GROUP BY recipient, sender;
