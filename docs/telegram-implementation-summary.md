# Telegram Local Bot API 集成说明

这份文档保留给开发者快速了解当前集成范围，不作为用户使用手册。

## 当前范围

- `crates/openfang-channels/src/telegram.rs`
  - 在未启用 Local Bot API 时，对超过 Telegram 官方 `getFile` 20MB 限制的文件给出明确告警。
  - 在启用 Local Bot API 时继续解析下载地址，并保留现有下载进度回调。
- `crates/openfang-api/src/channel_bridge.rs`
  - 读取 `[channels.telegram]` 下的 Local Bot API 配置。
  - 在 `use_local_api = true` 且 `auto_start_local_api = true` 时尝试启动本地 `telegram-bot-api`。
  - 自动启动成功且 `api_url` 未配置时，默认回落到 `http://127.0.0.1:<local_api_port>`。
- `crates/openfang-kernel/src/telegram_local_api.rs`
  - 管理 `telegram-bot-api` 子进程。
  - 支持二进制查找、启动、初始就绪检查、异常重启和停机清理。
- `crates/openfang-types/src/config.rs`
  - 暴露 `use_local_api`、`auto_start_local_api`、`telegram_api_id`、`telegram_api_hash_env`、`local_api_port` 等配置字段。

## 文档入口

- 用户使用手册：[telegram-large-files.md](telegram-large-files.md)
- 配置样例：[telegram-config-example.toml](telegram-config-example.toml)
- shipinbot 对接说明：[telegram-shipinbot-integration.md](telegram-shipinbot-integration.md)

## 说明

- 如果需要讨论后续功能规划，例如交互式下载确认、视频处理工具链或 shipinbot 自动提交流程，建议放到独立设计文档或 issue 中，不再混入这里。
