<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-03-29 | Updated: 2026-03-29 -->

# packages

## Purpose
Auxiliary packages for OpenFang — WhatsApp Web integration via Baileys library.

## Key Files
| File | Description |
|------|-------------|
| `whatsapp-gateway/index.js` | WhatsApp gateway implementation — QR login, bidirectional messaging |
| `whatsapp-gateway/package.json` | Node >=18, depends on Baileys, Pino logger, QR code lib |

## For AI Agents

### Working In This Directory
- WhatsApp gateway is a standalone Node.js service that bridges WhatsApp Web to OpenFang.
- Uses Baileys library for headless WhatsApp Web automation — QR code login, no phone emulation.
- Messages flow bidirectionally: WhatsApp → OpenFang agents and agent responses → WhatsApp.
- Pino is used for structured logging — configure appropriately in production.
- When updating dependencies, test against live WhatsApp Web to ensure Baileys compatibility.

<!-- MANUAL: -->
