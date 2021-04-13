DROP INDEX localplayerchat_by_timestamp;
DROP INDEX remoteplayerchat_by_timestamp;
DROP VIEW LastMessages;

ALTER TABLE Player DROP CONSTRAINT player_realm_id;

DROP TABLE Bookmark;
DROP TABLE ServerACL;
DROP TABLE RealmChat;
DROP TABLE Realm;
DROP TABLE LocalPlayerChat;
DROP TABLE RemotePlayerChat;
DROP TABLE Player;

DROP TABLE AuthOTP;
