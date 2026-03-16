# shipinfabu Hand - 内置版本

这是 shipinfabu Hand 的内置版本，编译时嵌入 OpenFang 二进制。

## 源仓库

主开发仓库位于：
```
/Users/xiaomo/Desktop/shipinbot/openfang-hand/shipinfabu/
```

## 维护说明

**⚠️ 不要直接在此目录修改文件！**

此目录的文件从主仓库同步而来。如需修改：

1. 在主仓库修改：`/Users/xiaomo/Desktop/shipinbot/openfang-hand/shipinfabu/`
2. 运行同步脚本：
   ```bash
   cd /Users/xiaomo/Desktop/shipinbot/openfang-hand/shipinfabu/
   ./sync-to-openfang.sh
   ```
3. 验证集成：
   ```bash
   cd /Users/xiaomo/Desktop/openfang-upstream-fork
   cargo test -p openfang-hands --lib
   ```

## 文件说明

- `HAND.toml` - Hand 定义（从主仓库同步）
- `SKILL.md` - Hand 技能文档（从主仓库同步）
- `README.md` - 本文档（仅内置版本有）

## 集成方式

此 Hand 通过 `bundled.rs` 的 `include_str!` 宏编译时嵌入：

```rust
(
    "shipinfabu",
    include_str!("../bundled/shipinfabu/HAND.toml"),
    include_str!("../bundled/shipinfabu/SKILL.md"),
),
```

## 相关文档

完整的双向维护指南请查看主仓库：
```
/Users/xiaomo/Desktop/shipinbot/openfang-hand/shipinfabu/SYNC.md
```
