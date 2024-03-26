use error_chain::error_chain;
error_chain! {
    foreign_links {
        Io(std::io::Error);
        HttpRequest(reqwest::Error);
        Json(serde_json::Error);
        Toml(toml::de::Error);
        JoinError(tokio::task::JoinError);
    }
}
