//! The one DNS query Skim makes: the MX lookup that tells onboarding whether a
//! domain's mail lives at Microsoft.
//!
//! Windows goes through the system resolver (`DnsQuery_W`), so it uses the
//! machine's own DNS servers and cache. Android has no system DNS API usable
//! from Rust, so it asks Cloudflare's DNS-over-HTTPS endpoint instead — the
//! queried domain still never reaches a mail-unrelated third party beyond the
//! resolver itself.

#[cfg(windows)]
use std::ffi::c_void;
#[cfg(windows)]
use std::ptr::NonNull;
#[cfg(windows)]
use windows::core::HSTRING;
#[cfg(windows)]
use windows::Win32::NetworkManagement::Dns::{
    DnsFree, DnsFreeRecordList, DnsQuery_W, DNS_QUERY_STANDARD, DNS_RECORDA, DNS_RECORDW,
    DNS_TYPE_MX,
};

/// The MX exchange hosts of `domain`, or an empty vec on any failure — no
/// records, NXDOMAIN, no network are all the same answer to the caller.
///
/// Blocking: the resolver call has no timeout of its own and a dead resolver
/// can sit on the call for seconds, so callers run this on `spawn_blocking`
/// under a `tokio::time::timeout`.
#[cfg(windows)]
pub fn mx_hosts(domain: &str) -> Vec<String> {
    let name = HSTRING::from(domain);
    let mut out: *mut DNS_RECORDA = std::ptr::null_mut();
    // SAFETY: `name` is a NUL-terminated wide string that outlives the call, and
    // `out` is a valid out-pointer. On success the resolver hands back a list it
    // owns, which goes back to `DnsFree` below.
    let status =
        unsafe { DnsQuery_W(&name, DNS_TYPE_MX, DNS_QUERY_STANDARD, None, &mut out, None) };
    // Everything past here works on `NonNull`, so the "the resolver gave us a
    // list" check is made once and carried by the type instead of by a comment.
    let Some(head) = NonNull::new(out).filter(|_| status.is_ok()) else {
        return Vec::new();
    };

    let mut hosts = Vec::new();
    // The `_W` query returns wide records; the out-parameter is typed
    // `DNS_RECORDA` for both charsets, and the layouts differ only in the string
    // type.
    let mut next = Some(head.cast::<DNS_RECORDW>());
    while let Some(node) = next {
        // SAFETY: the resolver owns this list and nothing frees it before the
        // `DnsFree` below, so every node it links to is live for this walk.
        let record = unsafe { node.as_ref() };
        // The list can carry other record types — a CNAME at the head, say — so
        // the `MX` arm of the union is only read once `wType` says it is an MX.
        if record.wType == DNS_TYPE_MX.0 {
            // SAFETY: `wType` above says this record's union holds MX data.
            let exchange = unsafe { record.Data.MX.pNameExchange };
            if !exchange.is_null() {
                // SAFETY: a non-null exchange name is a NUL-terminated wide
                // string owned by the same list.
                if let Ok(host) = unsafe { exchange.to_string() } {
                    hosts.push(host);
                }
            }
        }
        next = NonNull::new(record.pNext);
    }

    // SAFETY: this is the list `DnsQuery_W` handed out, freed exactly once, and
    // the strings above were copied into owned `String`s.
    unsafe { DnsFree(Some(head.as_ptr() as *const c_void), DnsFreeRecordList) };
    hosts
}

/// Android MX lookup over DNS-over-HTTPS (Cloudflare). Same contract as the
/// Windows resolver path: exchange hosts, or empty on any failure. Synchronous
/// with its own short budget — callers already run this on `spawn_blocking`.
#[cfg(target_os = "android")]
pub fn mx_hosts(domain: &str) -> Vec<String> {
    let url = format!(
        "https://cloudflare-dns.com/dns-query?name={}&type=MX",
        domain.trim_end_matches('.')
    );
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build();
    let Ok(client) = client else {
        return Vec::new();
    };
    let Ok(resp) = client
        .get(&url)
        .header("accept", "application/dns-json")
        .send()
    else {
        return Vec::new();
    };
    let Ok(json): Result<serde_json::Value, _> = resp.json() else {
        return Vec::new();
    };
    json.get("Answer")
        .and_then(|a| a.as_array())
        .map(|answers| {
            answers
                .iter()
                .filter(|a| a.get("type").and_then(|t| t.as_u64()) == Some(15))
                .filter_map(|a| {
                    a.get("data")
                        .and_then(|d| d.as_str())
                        // "10 contoso-com.mail.protection.outlook.com."
                        .and_then(|d| d.split_whitespace().last())
                        .map(|h| h.trim_end_matches('.').to_string())
                })
                .collect()
        })
        .unwrap_or_default()
}
