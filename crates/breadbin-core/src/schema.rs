// @generated automatically by Diesel CLI.

diesel::table! {
    _meta (key) {
        key -> Nullable<Text>,
        value -> Text,
    }
}

diesel::table! {
    events (id) {
        id -> Nullable<Integer>,
        name -> Text,
        event_type -> Nullable<Text>,
        aka -> Nullable<Text>,
        start_day -> Nullable<Integer>,
        start_month -> Nullable<Integer>,
        start_year -> Nullable<Integer>,
        end_day -> Nullable<Integer>,
        end_month -> Nullable<Integer>,
        end_year -> Nullable<Integer>,
        address -> Nullable<Text>,
        zip -> Nullable<Text>,
        city -> Nullable<Text>,
        country -> Nullable<Text>,
        website -> Nullable<Text>,
        updated_at -> Nullable<Timestamp>,
    }
}

diesel::table! {
    groups (id) {
        id -> Nullable<Integer>,
        name -> Text,
        short -> Nullable<Text>,
        base_country -> Nullable<Text>,
        founded_month -> Nullable<Integer>,
        founded_year -> Nullable<Integer>,
        website -> Nullable<Text>,
        rating -> Nullable<Float>,
        grouptypes -> Nullable<Text>,
        slogans -> Nullable<Text>,
        updated_at -> Nullable<Timestamp>,
    }
}

diesel::table! {
    ia_mappings (id) {
        id -> Nullable<Integer>,
        release_id -> Integer,
        ia_identifier -> Text,
        ia_title -> Nullable<Text>,
        ia_collection -> Nullable<Text>,
        match_confidence -> Nullable<Float>,
        last_checked -> Nullable<Timestamp>,
    }
}

diesel::table! {
    release_credits (id) {
        id -> Nullable<Integer>,
        release_id -> Integer,
        scener_id -> Integer,
        handle -> Text,
        credit_type -> Text,
        position -> Nullable<Integer>,
        functions -> Nullable<Text>,
    }
}

diesel::table! {
    release_downloads (id) {
        id -> Nullable<Integer>,
        release_id -> Integer,
        url -> Text,
        counter_url -> Nullable<Text>,
        downloads -> Nullable<Integer>,
        status -> Nullable<Text>,
        is_internal -> Nullable<Bool>,
        file_type -> Nullable<Text>,
    }
}

diesel::table! {
    release_sids (id) {
        id -> Nullable<Integer>,
        release_id -> Integer,
        sid_id -> Nullable<Integer>,
        hvsc_path -> Nullable<Text>,
        name -> Text,
        author -> Nullable<Text>,
        released -> Nullable<Text>,
        load_addr -> Nullable<Integer>,
        init_addr -> Nullable<Integer>,
        play_addr -> Nullable<Integer>,
        num_songs -> Nullable<Integer>,
        default_song -> Nullable<Integer>,
        sid_model -> Nullable<Text>,
        clock_speed -> Nullable<Text>,
        data_size -> Nullable<Integer>,
    }
}

diesel::table! {
    release_types (id) {
        id -> Nullable<Integer>,
        name -> Text,
        description -> Nullable<Text>,
    }
}

diesel::table! {
    releases (id) {
        id -> Nullable<Integer>,
        name -> Text,
        release_type_id -> Integer,
        group_id -> Nullable<Integer>,
        rating -> Nullable<Float>,
        release_day -> Nullable<Integer>,
        release_month -> Nullable<Integer>,
        release_year -> Nullable<Integer>,
        event_id -> Nullable<Integer>,
        compo -> Nullable<Text>,
        place -> Nullable<Integer>,
        screenshot_url -> Nullable<Text>,
        screenshot_urls -> Nullable<Text>,
        download_count -> Nullable<Integer>,
        website -> Nullable<Text>,
        created_at -> Nullable<Timestamp>,
        updated_at -> Nullable<Timestamp>,
    }
}

diesel::table! {
    scener_handles (id) {
        id -> Nullable<Integer>,
        scener_id -> Integer,
        handle -> Text,
        aka -> Nullable<Text>,
        currently_used -> Nullable<Bool>,
        handle_story -> Nullable<Text>,
    }
}

diesel::table! {
    sceners (id) {
        id -> Nullable<Integer>,
        country -> Nullable<Text>,
        trivia -> Nullable<Text>,
        handle_story -> Nullable<Text>,
        updated_at -> Nullable<Timestamp>,
    }
}

diesel::joinable!(ia_mappings -> releases (release_id));
diesel::joinable!(release_credits -> releases (release_id));
diesel::joinable!(release_credits -> sceners (scener_id));
diesel::joinable!(release_downloads -> releases (release_id));
diesel::joinable!(release_sids -> releases (release_id));
diesel::joinable!(releases -> events (event_id));
diesel::joinable!(releases -> groups (group_id));
diesel::joinable!(releases -> release_types (release_type_id));
diesel::joinable!(scener_handles -> sceners (scener_id));

diesel::allow_tables_to_appear_in_same_query!(
    _meta,
    events,
    groups,
    ia_mappings,
    release_credits,
    release_downloads,
    release_sids,
    release_types,
    releases,
    scener_handles,
    sceners,
);
