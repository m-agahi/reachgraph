//! `reachgraph merge` — ADR-0010.
//!
//! Reads several per-repository artifacts, joins them on `join_key` through
//! [`reachgraph_core::estate::merge`], and writes two files: `estate.json`, the
//! join as data, and `estate.html`, one self-contained page. The page needs no
//! script and no server: every drill-down is a `<details>` element, and every
//! link from a consumed RPC to the handler that serves it is an in-page anchor.
//!
//! Nothing is analysed here. The merge reads what runs already wrote, so it is
//! exactly as current as the oldest artifact handed to it.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use reachgraph_core::estate::{
    merge, EstateDocument, JoinRow, JoinStatus, NodeCard, RepoArtifact, SideRow,
};
use reachgraph_core::schema::{EndpointsDocument, ShardDocument};

use crate::{Streams, EXIT_INTERNAL, EXIT_OK, EXIT_USAGE};

/// The two files the merge owns.
const OWNED: [&str; 2] = ["estate.json", "estate.html"];

/// How deep a trace follows join keys before it stops and says so.
const TRACE_DEPTH: usize = 8;

/// Run the subcommand.
pub fn subcommand(inputs: &[String], out: &Path, force: bool, streams: &mut Streams<'_>) -> u8 {
    let mut repos = Vec::new();
    for input in inputs {
        match read_input(input) {
            Ok(repo) => repos.push(repo),
            Err(message) => {
                let _ = writeln!(streams.err, "error: {message}");
                return EXIT_USAGE;
            }
        }
    }

    if !force {
        let present: Vec<&str> = OWNED
            .iter()
            .copied()
            .filter(|name| out.join(name).exists())
            .collect();
        if !present.is_empty() {
            let _ = writeln!(
                streams.err,
                "error: {} already holds {}; pass --force to overwrite",
                out.display(),
                present.join(" and ")
            );
            return EXIT_USAGE;
        }
    }

    let document = merge(&repos);
    let json = match serde_json::to_string_pretty(&document) {
        Ok(json) => json,
        Err(error) => {
            let _ = writeln!(
                streams.err,
                "error: could not serialise the estate: {error}"
            );
            return EXIT_INTERNAL;
        }
    };
    let written = std::fs::create_dir_all(out)
        .and_then(|()| std::fs::write(out.join("estate.json"), json + "\n"))
        .and_then(|()| std::fs::write(out.join("estate.html"), render(&document)));
    if let Err(error) = written {
        let _ = writeln!(
            streams.err,
            "error: could not write {}: {error}",
            out.display()
        );
        return EXIT_INTERNAL;
    }

    let joined = document
        .joins
        .iter()
        .filter(|join| join.status == JoinStatus::Joined)
        .count();
    let _ = writeln!(
        streams.err,
        "merged {} repositories: {} join keys, {joined} joined; wrote {}",
        document.repos.len(),
        document.joins.len(),
        out.display()
    );
    EXIT_OK
}

/// One input: `[label=]path`, where path is an `endpoints.json` or the
/// directory holding one.
fn read_input(input: &str) -> Result<RepoArtifact, String> {
    let (label, path) = match input.split_once('=') {
        Some((label, path)) if !label.is_empty() && !label.contains(['/', '\\']) => {
            (Some(label.to_owned()), PathBuf::from(path))
        }
        _ => (None, PathBuf::from(input)),
    };
    let endpoints_path = if path.is_dir() {
        path.join("endpoints.json")
    } else {
        path.clone()
    };
    let directory = endpoints_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();

    let endpoints: EndpointsDocument = read_json(&endpoints_path)?;
    let mut shards = BTreeMap::new();
    for version in endpoints.operations.iter().flat_map(|op| &op.versions) {
        if let Some(shard) = &version.shard {
            let document: ShardDocument = read_json(&directory.join(shard))?;
            shards.insert(shard.clone(), document);
        }
    }

    let label = label.unwrap_or_else(|| {
        std::fs::canonicalize(&directory)
            .unwrap_or(directory.clone())
            .file_name()
            .map_or_else(
                || directory.display().to_string(),
                |name| name.to_string_lossy().into_owned(),
            )
    });
    Ok(RepoArtifact {
        label,
        endpoints,
        shards,
    })
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    serde_json::from_str(&text)
        .map_err(|error| format!("{} is not a reachgraph artifact: {error}", path.display()))
}

// ---------------------------------------------------------------- the page

/// The in-page anchor for a join key.
fn anchor(key: &str) -> String {
    let slug: String = key
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    format!("join-{slug}")
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

fn status_label(join: &JoinRow) -> String {
    match join.status {
        JoinStatus::Joined => "joined".to_owned(),
        JoinStatus::ConsumedNotServed => {
            "consumed — not served by any repository in this merge".to_owned()
        }
        JoinStatus::ServedNotConsumed => {
            "served — not consumed by any repository in this merge".to_owned()
        }
        JoinStatus::AmbiguousServed => format!(
            "served by {} repositories — not resolved by preference",
            join.served.len()
        ),
    }
}

fn status_class(status: JoinStatus) -> &'static str {
    match status {
        JoinStatus::Joined => "ok",
        JoinStatus::ConsumedNotServed | JoinStatus::ServedNotConsumed => "gap",
        JoinStatus::AmbiguousServed => "warn",
    }
}

/// One node as a box. A missing doc comment is said, never left blank; a
/// consumed node with none borrows the serving handler's, and says whose.
fn node_box(repo: &str, card: &NodeCard, borrowed: Option<(&str, &NodeCard)>) -> String {
    let mut html = String::from("<div class=\"box\">");
    let _ = write!(
        html,
        "<div class=\"repo\">{}</div><div class=\"name\">{}</div>",
        escape(repo),
        escape(&card.name)
    );
    if let Some(file) = &card.file {
        let _ = write!(html, "<div class=\"file\">{}</div>", escape(file));
    }
    match (&card.doc, borrowed) {
        (Some(doc), _) => {
            let _ = write!(html, "<div class=\"doc\">{}</div>", escape(doc.trim()));
        }
        (None, Some((server, handler))) => {
            let _ = write!(
                html,
                "<div class=\"doc none\">no doc comment in source</div>\
                 <div class=\"doc borrowed\"><span>served by {} as <code>{}</code>:</span> {}</div>",
                escape(server),
                escape(&handler.name),
                handler.doc.as_deref().map_or_else(
                    || "<em>no doc comment there either</em>".to_owned(),
                    |doc| escape(doc.trim())
                )
            );
        }
        (None, None) => html.push_str("<div class=\"doc none\">no doc comment in source</div>"),
    }
    html.push_str("</div>");
    html
}

fn unbound_box(side: &SideRow) -> String {
    format!(
        "<div class=\"box unbound\"><div class=\"repo\">{}</div><div class=\"name\">{}/{}</div>\
         <div class=\"doc none\">not bound: {}</div></div>",
        escape(&side.repo),
        escape(&side.service),
        escape(&side.operation),
        escape(
            side.unbound_reason
                .as_deref()
                .unwrap_or("no reason recorded")
        )
    )
}

fn side_box(side: &SideRow, borrowed: Option<(&str, &NodeCard)>) -> String {
    match &side.node {
        Some(card) => node_box(&side.repo, card, borrowed),
        None => unbound_box(side),
    }
}

fn key_link(join: &JoinRow) -> String {
    format!(
        "<a class=\"key {}\" href=\"#{}\">{}</a> <span class=\"status {}\">{}</span>",
        status_class(join.status),
        anchor(&join.join_key),
        escape(&join.join_key),
        status_class(join.status),
        escape(&status_label(join))
    )
}

fn shard_limit(side: &SideRow) -> Option<String> {
    let limit = side.depth_limit?;
    Some(if side.frontier_count == 0 {
        format!("shard depth limit {limit}; nothing was cut")
    } else {
        format!(
            "shard depth limit {limit}: {} node(s) cut, so calls beyond them are unmeasured",
            side.frontier_count
        )
    })
}

/// A served side and everything it reaches, recursively, as nested lists.
fn trace_served(
    side: &SideRow,
    joins: &BTreeMap<&str, &JoinRow>,
    seen: &mut BTreeSet<String>,
    depth: usize,
    html: &mut String,
) {
    html.push_str(&side_box(side, None));
    if let Some(limit) = shard_limit(side) {
        let _ = write!(html, "<div class=\"limit\">{}</div>", escape(&limit));
    }
    if side.reaches.is_empty() {
        return;
    }
    html.push_str("<ul class=\"trace\">");
    for reach in &side.reaches {
        html.push_str("<li><div class=\"path\">");
        // The handler is already drawn; draw the steps after it.
        for step in reach.path.iter().skip(1) {
            html.push_str(&node_box(&side.repo, step, None));
        }
        html.push_str("</div>");
        match joins.get(reach.join_key.as_str()) {
            Some(join) => {
                let _ = write!(html, "<div class=\"hop\">→ {}</div>", key_link(join));
                if depth >= TRACE_DEPTH {
                    html.push_str("<div class=\"limit\">trace depth limit reached</div>");
                } else if !seen.insert(reach.join_key.clone()) {
                    html.push_str("<div class=\"limit\">already traced above (cycle)</div>");
                } else {
                    for served in &join.served {
                        trace_served(served, joins, seen, depth + 1, html);
                    }
                    seen.remove(&reach.join_key);
                }
            }
            None => {
                let _ = write!(
                    html,
                    "<div class=\"hop\">→ {}</div>",
                    escape(&reach.join_key)
                );
            }
        }
        html.push_str("</li>");
    }
    html.push_str("</ul>");
}

/// Consumed sides no served handler in the same repository reaches: where a
/// repository's own traffic leaves it, the place a trace starts.
fn entry_sides(document: &EstateDocument) -> Vec<(&JoinRow, &SideRow)> {
    let reached: BTreeSet<(&str, &str)> = document
        .joins
        .iter()
        .flat_map(|join| &join.served)
        .flat_map(|side| {
            side.reaches
                .iter()
                .map(move |reach| (side.repo.as_str(), reach.join_key.as_str()))
        })
        .collect();
    document
        .joins
        .iter()
        .filter(|join| join.status == JoinStatus::Joined)
        .flat_map(|join| join.consumed.iter().map(move |side| (join, side)))
        .filter(|(join, side)| !reached.contains(&(side.repo.as_str(), join.join_key.as_str())))
        .collect()
}

fn render(document: &EstateDocument) -> String {
    let joins: BTreeMap<&str, &JoinRow> = document
        .joins
        .iter()
        .map(|join| (join.join_key.as_str(), join))
        .collect();

    let mut html = String::new();
    html.push_str(HEAD);
    let _ = write!(
        html,
        "<h1>Estate call graph</h1><p class=\"claim\">{}.</p>",
        escape(&document.claim)
    );

    html.push_str("<h2>Repositories</h2><table><tr><th>repository</th><th>roots bound</th><th>limits of its run</th></tr>");
    for repo in &document.repos {
        let notes: String = repo
            .notes
            .iter()
            .map(|note| format!("<li>{}</li>", escape(note)))
            .collect();
        let _ = write!(
            html,
            "<tr><td>{}</td><td>{} / {}{}</td><td><ul>{}</ul></td></tr>",
            escape(&repo.label),
            repo.roots_bound,
            repo.roots_total,
            if repo.partial { " (partial)" } else { "" },
            notes
        );
    }
    html.push_str("</table>");

    html.push_str(
        "<h2>Traces</h2><p class=\"hint\">From each consumed RPC that no served handler \
         in its own repository reaches, through the handler that serves it, and on through \
         every RPC that handler's recorded calls reach.</p>",
    );
    let entries = entry_sides(document);
    if entries.is_empty() {
        html.push_str("<p class=\"limit\">no joined key starts a trace in this merge</p>");
    }
    for (join, side) in entries {
        let server = join.served.first();
        let borrowed = server.and_then(|served| {
            served
                .node
                .as_ref()
                .map(|node| (served.repo.as_str(), node))
        });
        html.push_str("<section class=\"trace-root\">");
        html.push_str(&side_box(side, borrowed));
        let _ = write!(html, "<div class=\"hop\">→ {}</div>", key_link(join));
        let mut seen = BTreeSet::from([join.join_key.clone()]);
        for served in &join.served {
            trace_served(served, &joins, &mut seen, 1, &mut html);
        }
        html.push_str("</section>");
    }

    html.push_str(
        "<h2>Join keys</h2><table><tr><th>join key</th><th>served by</th><th>consumed by</th></tr>",
    );
    for join in &document.joins {
        let repos = |sides: &[SideRow]| {
            sides
                .iter()
                .map(|side| escape(&side.repo))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let _ = write!(
            html,
            "<tr><td>{}</td><td>{}</td><td>{}</td></tr>",
            key_link(join),
            repos(&join.served),
            repos(&join.consumed)
        );
    }
    html.push_str("</table>");

    html.push_str("<h2>Drill-down</h2>");
    for join in &document.joins {
        let _ = write!(
            html,
            "<details id=\"{}\"><summary>{} <span class=\"status {}\">{}</span></summary>",
            anchor(&join.join_key),
            escape(&join.join_key),
            status_class(join.status),
            escape(&status_label(join))
        );
        for side in &join.served {
            html.push_str("<h3>served</h3>");
            html.push_str(&side_box(side, None));
            if let Some(limit) = shard_limit(side) {
                let _ = write!(html, "<div class=\"limit\">{}</div>", escape(&limit));
            }
            for reach in &side.reaches {
                let reached = joins.get(reach.join_key.as_str());
                let _ = write!(
                    html,
                    "<div class=\"hop\">reaches {}</div><div class=\"path\">",
                    reached.map_or_else(|| escape(&reach.join_key), |join| key_link(join))
                );
                for step in &reach.path {
                    html.push_str(&node_box(&side.repo, step, None));
                }
                html.push_str("</div>");
            }
            if !side.shard_nodes.is_empty() {
                let _ = write!(
                    html,
                    "<details class=\"nodes\"><summary>{} described node(s) in this shard</summary>",
                    side.shard_nodes.len()
                );
                for card in &side.shard_nodes {
                    html.push_str(&node_box(&side.repo, card, None));
                }
                html.push_str("</details>");
            }
        }
        let borrowed = join.served.first().and_then(|served| {
            served
                .node
                .as_ref()
                .map(|node| (served.repo.as_str(), node))
        });
        for side in &join.consumed {
            html.push_str("<h3>consumed</h3>");
            html.push_str(&side_box(side, borrowed));
        }
        html.push_str("</details>");
    }

    html.push_str("</main></body></html>\n");
    html
}

const HEAD: &str = r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Estate call graph</title>
<style>
:root{--bg:#fbfbf9;--fg:#1d1d1b;--muted:#6b6b66;--line:#d9d8d2;--box:#ffffff;--ok:#2f6f3e;--gap:#9a5b00;--warn:#a0302a;--accent:#2a5d9f}
@media (prefers-color-scheme: dark){:root{--bg:#161615;--fg:#e9e8e3;--muted:#a3a29b;--line:#3a3935;--box:#1f1f1d;--ok:#7cc48b;--gap:#e0a64a;--warn:#ec8a83;--accent:#8ab4ec}}
body{margin:0;background:var(--bg);color:var(--fg);font:15px/1.5 system-ui,sans-serif}
main{max-width:1100px;margin:0 auto;padding:16px}
h1{font-size:1.6rem;margin:.5rem 0}h2{margin-top:2rem;border-bottom:1px solid var(--line)}h3{font-size:.9rem;color:var(--muted);text-transform:uppercase;margin:.75rem 0 .25rem}
.claim,.hint{color:var(--muted)}
table{border-collapse:collapse;width:100%;font-size:.9rem;display:block;overflow-x:auto}td,th{border-bottom:1px solid var(--line);padding:.35rem .5rem;text-align:left;vertical-align:top}
ul{margin:0;padding-left:1.1rem}
.box{background:var(--box);border:1px solid var(--line);border-radius:6px;padding:.5rem .7rem;margin:.35rem 0;max-width:640px}
.box .repo{font-size:.75rem;color:var(--muted);text-transform:uppercase;letter-spacing:.04em}
.box .name{font-family:ui-monospace,monospace;font-weight:600}
.box .file{font-family:ui-monospace,monospace;font-size:.75rem;color:var(--muted);word-break:break-all}
.box .doc{white-space:pre-wrap;margin-top:.25rem}.box .doc.none{color:var(--muted);font-style:italic}
.box .doc.borrowed span{color:var(--muted);font-size:.85rem}
.box.unbound{border-style:dashed}
.trace{list-style:none;border-left:2px solid var(--line);margin-left:.6rem;padding-left:1rem}
.trace-root{margin:1rem 0;padding-bottom:1rem;border-bottom:1px dotted var(--line)}
.path{display:flex;flex-wrap:wrap;gap:.4rem}.path .box{flex:1 1 220px}
.hop{margin:.3rem 0;font-family:ui-monospace,monospace}
a.key{color:var(--accent);word-break:break-all}
.status{font-size:.75rem;padding:.05rem .4rem;border-radius:3px;border:1px solid currentColor}
.status.ok{color:var(--ok)}.status.gap{color:var(--gap)}.status.warn{color:var(--warn)}
.limit{color:var(--gap);font-size:.85rem}
details{border:1px solid var(--line);border-radius:6px;padding:.4rem .7rem;margin:.4rem 0}details>summary{cursor:pointer;font-family:ui-monospace,monospace;word-break:break-all}
code{font-family:ui-monospace,monospace}
</style></head><body><main>
"#;
