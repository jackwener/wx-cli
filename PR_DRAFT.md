# feat(biz): add `wx biz-articles` command to query public account messages

## Summary

Adds a new `biz-articles` subcommand that queries locally cached WeChat public account (公众号) article pushes from `biz_message_0.db`.

This enables a downstream workflow for downloading full article content:

```bash
wx biz-articles --since today --json | jq '.[].url' | xargs opencli weixin download
```

## Background

- WeChat stores public account (官方账号) message pushes in a separate database: `message/biz_message_0.db` (SQLCipher 4 encrypted)
- This DB was not exposed by any existing wx-cli command
- The encryption key is already scanned and stored in `~/.wx-cli/all_keys.json` by `wx init`
- Each public account has its own `Msg_{md5(username)}` table, following the same convention as `message_0.db`
- Message content is zstd-compressed XML containing `<mmreader>/<item>` structures with article metadata

## New CLI Interface

```bash
# Last 50 articles (default)
wx biz-articles

# More articles
wx biz-articles -n 200

# Filter by public account name (fuzzy match on display name)
wx biz-articles --account "返朴"
wx biz-articles --account "Datawhale"

# Time filter (article publish time, YYYY-MM-DD)
wx biz-articles --since 2026-05-10
wx biz-articles --since 2026-05-01 --until 2026-05-10

# JSON output (for downstream piping)
wx biz-articles --json
wx biz-articles --since 2026-05-10 --json | jq '.[].url'
```

## Output Fields

Each article item includes:

| Field | Description |
|-------|-------------|
| `time` | Article publish time (formatted) |
| `timestamp` | Article publish timestamp (seconds) |
| `recv_time` | Message receive time (when WeChat pushed it) |
| `recv_time_str` | Message receive time (formatted) |
| `account` | Public account display name |
| `account_username` | Public account username (gh_*) |
| `title` | Article title |
| `url` | Article URL (mp.weixin.qq.com link) |
| `digest` | Article summary/excerpt |
| `cover_url` | Cover image URL |

## Implementation Notes

- `biz_message_0.db` is loaded on-demand via existing `DbCache` mechanism (no startup cost unless `biz-articles` is called)
- The key for `message/biz_message_0.db` is already in `all_keys.json`, no changes to `wx init` needed
- Multi-article pushes (图文消息) are expanded: each `<item>` in `<mmreader>` becomes a separate output row
- Items without URL or title (e.g., payment notifications from service accounts) are filtered out
- New `extract_cdata` helper function strips CDATA wrappers from XML content
- Results sorted by `pub_time` DESC (article publish time, not message receive time)

## Changes

- `src/ipc.rs`: Add `BizArticles` IPC request variant
- `src/cli/biz_articles.rs`: New CLI command handler (follows sns_feed pattern)  
- `src/cli/mod.rs`: Register `BizArticles` subcommand in clap + dispatch
- `src/daemon/query.rs`: Add `q_biz_articles` query + `parse_biz_xml_items` + `extract_cdata` helpers + 8 unit tests
- `src/daemon/server.rs`: Add dispatch case for `BizArticles`

## Test Results

```
test result: ok. 49 passed; 0 failed; 0 ignored
```

New tests (8):
- `biz_tests::extract_cdata_normal`
- `biz_tests::extract_cdata_empty`
- `biz_tests::extract_cdata_url`
- `biz_tests::extract_cdata_no_cdata_wrapper`
- `biz_tests::parse_biz_xml_items_single_article`
- `biz_tests::parse_biz_xml_items_skips_no_url`
- `biz_tests::parse_biz_xml_items_multi_article`
- `biz_tests::parse_biz_xml_items_pub_time_fallback`

## Verified Output (real WeChat install with ~30 public accounts, 2026-05-10)

```yaml
- account: 返朴
  title: 细胞生物学家俞立：从后进生到科学家，一个ADHD孩子的逆袭
  url: http://mp.weixin.qq.com/s?__biz=Mzg2MTUyODU2NA==&mid=2247642795&...

- account: Datawhale
  title: 刚刚，Claude Code 团队这篇文章爆了！
  url: http://mp.weixin.qq.com/s?__biz=MzIyNjM2MzQyNg==&mid=2247722630&...

- account: 土猛的员外
  title: AI时代，企业的业务底座正在从数据库变成知识引擎
  url: http://mp.weixin.qq.com/s?__biz=MzIyOTA5NTM1OA==&mid=2247485270&...
```

## Branch

`ChenyqThu/wx-cli` → `feat/biz-articles`

---

*Waiting for Lucien's review before opening PR.*
