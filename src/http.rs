use std::time::Duration;

/// One agent for the whole run: connect 1 s, total 3 s, matching the
/// `curl --connect-timeout 1 --max-time 3` used by the previous dashboard.
pub fn build_agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(1)))
        .timeout_global(Some(Duration::from_secs(3)))
        .build()
        .new_agent()
}

/// `GET {base_url}/status`. Transport errors, non-2xx answers and body read
/// failures all mean "offline": no status text.
pub fn fetch_status(agent: &ureq::Agent, base_url: &str) -> Option<String> {
    let url = format!("{base_url}/status");
    let mut response = agent.get(url).call().ok()?;
    if !response.status().is_success() {
        return None;
    }
    let bytes = response.body_mut().read_to_vec().ok()?;
    String::from_utf8(bytes).ok()
}
