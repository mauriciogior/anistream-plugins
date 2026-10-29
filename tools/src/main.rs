use anistream_plugin::{Limits, PluginHost};
use anistream_store_core::manifest::{self, PluginInfo};
use anyhow::{Context, Result, ensure};

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    ensure!(
        args.len() >= 2,
        "usage: tools INPUT.wasm STAMPED.wasm [--live QUERY]"
    );
    let http = anistream_net::HttpClient::new(&Default::default())?;
    let host = PluginHost::new(Limits::default(), Some(http.clone()))?;
    let plugin = host
        .load(&args[0])
        .await
        .context("load real WASM component")?;
    let m = plugin.describe().await?;
    ensure!(m.id == "ani-cli", "wrong plugin ID");
    ensure!(
        m.allowed_hosts == ["hianime.at", "zokoanime.video"],
        "unexpected permissions"
    );
    ensure!(m.translation_types == ["sub", "dub"], "wrong translations");
    ensure!(
        plugin
            .sources("unused", "1", "sub")
            .await?
            .map_err(|e| anyhow::anyhow!("{e:?}"))?
            .is_empty(),
        "single-source plugin should have no source picker"
    );
    ensure!(
        plugin
            .resolve_source("unused", "1", "sub", "unknown")
            .await?
            .is_err(),
        "unknown source must be refused"
    );
    ensure!(
        plugin.search("unused", "invalid").await?.is_err(),
        "invalid translation must be refused"
    );
    let info = PluginInfo {
        id: m.id,
        name: m.display_name,
        version: m.version,
        allowed_hosts: m.allowed_hosts,
        translations: m.translation_types,
    };
    let raw = std::fs::read(&args[0])?;
    let stamped = manifest::stamp(&raw, &info)?;
    ensure!(
        manifest::read(&stamped)? == Some(info.clone()),
        "manifest stamp round trip"
    );
    std::fs::write(&args[1], &stamped)?;
    let reloaded = host.load(&args[1]).await?;
    ensure!(
        reloaded.describe().await?.version == info.version,
        "stamped component does not load"
    );
    println!(
        "WASM ABI, manifest, source selection and error checks passed ({} bytes)",
        stamped.len()
    );
    if let Some(pos) = args.iter().position(|s| s == "--live") {
        let query = args.get(pos + 1).context("--live requires a query")?;
        let hits = reloaded
            .search(query, "sub")
            .await?
            .map_err(|e| anyhow::anyhow!("search: {e:?}"))?;
        ensure!(!hits.is_empty(), "live search was empty");
        let mut resolved = false;
        for hit in hits.iter().take(5) {
            let episodes = reloaded
                .list_episodes(&hit.id, "sub")
                .await?
                .map_err(|e| anyhow::anyhow!("episodes: {e:?}"))?;
            let Some(first) = episodes.first() else {
                continue;
            };
            match reloaded.resolve(&hit.id, &first.number, "sub").await? {
                Ok(streams) if !streams.is_empty() => {
                    let stream = &streams[0];
                    ensure!(stream.kind == "hls", "expected HLS");
                    let mut request = http.plain().get(&stream.url);
                    for (name, value) in &stream.headers {
                        request = request.header(name, value);
                    }
                    let response = request.send().await?;
                    ensure!(response.status().is_success(), "playlist HTTP failure");
                    ensure!(
                        response.text().await?.trim_start().starts_with("#EXTM3U"),
                        "invalid HLS playlist"
                    );
                    println!(
                        "Live WASM: {} hits, {} episodes, {} streams, {} subtitles; HLS playlist valid. No video segments fetched.",
                        hits.len(),
                        episodes.len(),
                        streams.len(),
                        stream.subtitles.len()
                    );
                    resolved = true;
                    break;
                }
                Err(error) => eprintln!("Sample unavailable: {error:?}"),
                _ => {}
            }
        }
        ensure!(resolved, "no sample resolved");
    }
    Ok(())
}
