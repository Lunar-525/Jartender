//! Shared proxy policy for metadata, checksums, artifacts, and Java installers.
use crate::{Error, Result};
use std::{fmt, time::Duration};
use ureq::{Proxy, ProxyProtocol};

#[cfg(test)]
pub(crate) mod tests;

/// Proxy selection is explicit for library hosts; no configuration files are read.
#[derive(Clone, Default)]
pub enum ProxyConfig {
    /// Use ureq's ALL_PROXY, HTTPS_PROXY, HTTP_PROXY and NO_PROXY environment policy.
    #[default]
    Environment,
    /// Connect directly, ignoring proxy environment variables.
    Direct,
    /// A validated proxy. Prefer constructing this with [`Self::new`].
    Proxy(Proxy),
}

impl fmt::Debug for ProxyConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Environment => f.write_str("Environment"),
            Self::Direct => f.write_str("Direct"),
            Self::Proxy(proxy) => f
                .debug_struct("Proxy")
                .field("protocol", &proxy.protocol())
                .field("host", &proxy.host())
                .field("port", &proxy.port())
                .finish(),
        }
    }
}

impl ProxyConfig {
    /// Parse an HTTP, HTTPS, SOCKS5 or SOCKS5h proxy URL. Errors never include credentials.
    pub fn new(url: &str) -> Result<Self> {
        let invalid = || {
            Error::invalid(
                "proxy",
                "expected http://, https://, socks5:// or socks5h://HOST:PORT (optional user:password@)",
            )
        };
        let proxy = Proxy::new(url).map_err(|_| invalid())?;
        // http::Uri treats an out-of-range port as absent, so check its raw text.
        let authority = proxy.uri().authority().ok_or_else(invalid)?.as_str();
        let address = authority.rsplit('@').next().ok_or_else(invalid)?;
        let port = if address.starts_with('[') {
            address
                .split_once(']')
                .and_then(|(_, rest)| rest.strip_prefix(':'))
        } else {
            address.split_once(':').map(|(_, port)| port)
        };
        if !url.contains("://")
            || proxy.host().is_empty()
            || url.contains('#')
            || url.chars().any(char::is_whitespace)
            || port.is_some_and(|port| port.parse::<u16>().ok().filter(|p| *p != 0).is_none())
            || !matches!(
                proxy.protocol(),
                ProxyProtocol::Http
                    | ProxyProtocol::Https
                    | ProxyProtocol::Socks5
                    | ProxyProtocol::Socks5h
            )
            || !matches!(proxy.uri().path(), "" | "/")
            || proxy.uri().query().is_some()
            || proxy.port() == 0
        {
            return Err(invalid());
        }
        Ok(Self::Proxy(proxy))
    }

    pub(crate) fn agent(&self, timeout: Duration) -> ureq::Agent {
        let proxy = match self {
            Self::Environment => Proxy::try_from_env(),
            Self::Direct => None,
            Self::Proxy(proxy) => Some(proxy.clone()),
        };
        ureq::Agent::config_builder()
            .proxy(proxy)
            .user_agent(concat!(
                "Jartender/",
                env!("CARGO_PKG_VERSION"),
                " (https://github.com/Lunar-525/Jartender)"
            ))
            .timeout_global(Some(timeout))
            .build()
            .into()
    }
}

/// Java does not consume HTTP_PROXY/ALL_PROXY. Pass the agent's effective policy
/// as JVM properties, before -jar, so installer library downloads use it too.
pub(crate) fn installer_proxy_args(agent: &ureq::Agent) -> Result<Vec<String>> {
    let mut args = vec![
        "-Djava.net.useSystemProxies=false".into(),
        "-Dhttp.proxyHost=".into(),
        "-Dhttps.proxyHost=".into(),
        "-DsocksProxyHost=".into(),
    ];
    let Some(proxy) = agent.config().proxy() else {
        return Ok(args);
    };
    if proxy.username().is_some() || proxy.password().is_some() {
        return Err(Error::invalid(
            "proxy",
            "Forge/NeoForge installers require a proxy without authentication",
        ));
    }
    match proxy.protocol() {
        ProxyProtocol::Http => {
            for scheme in ["http", "https"] {
                args.push(format!("-D{scheme}.proxyHost={}", proxy.host()));
                args.push(format!("-D{scheme}.proxyPort={}", proxy.port()));
            }
        }
        ProxyProtocol::Socks5 | ProxyProtocol::Socks5h => {
            args.push(format!("-DsocksProxyHost={}", proxy.host()));
            args.push(format!("-DsocksProxyPort={}", proxy.port()));
            args.push("-DsocksProxyVersion=5".into());
        }
        _ => {
            return Err(Error::invalid(
                "proxy",
                "Forge/NeoForge installers support http:// or socks5:// proxies",
            ));
        }
    }
    // Explicit proxies route every host through the proxy, including loopback.
    // Environment proxies retain the usual comma-separated NO_PROXY exclusions.
    let bypass = if proxy.is_from_env() {
        std::env::var("NO_PROXY")
            .or_else(|_| std::env::var("no_proxy"))
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .flat_map(|host| match host.strip_prefix('.') {
                Some(domain) => vec![domain.to_owned(), format!("*.{domain}")],
                None => vec![host.to_owned()],
            })
            .collect::<Vec<_>>()
            .join("|")
    } else {
        String::new()
    };
    args.push(format!("-Dhttp.nonProxyHosts={bypass}"));
    args.push(format!("-DsocksNonProxyHosts={bypass}"));
    Ok(args)
}
