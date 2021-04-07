table! {
    lastmessage(id, principal) {
        id -> Int4,
        principal -> Text,
        last_time -> Timestamptz,
    }
}
