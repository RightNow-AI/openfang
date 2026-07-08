FreEco.ai — Portable Edition
============================

No installation needed. Everything FreEco.ai needs lives in this folder,
including your config, agents, and data (in the "data" subfolder) — so
you can carry this folder on a USB drive between computers.

How to run:

  Windows   -> double-click run-windows.bat
  macOS     -> double-click run-macos.command
               (first time: right-click -> Open, to bypass Gatekeeper)
  Linux     -> double-click run-linux.sh, or run ./run-linux.sh
               in a terminal if your file manager opens it as text instead

On first run, a setup wizard will ask for an LLM provider API key
(Groq, Anthropic, OpenAI, ...). After that, FreEco.ai starts and opens
its dashboard at http://127.0.0.1:4200 automatically.

Your data stays in the "data" folder next to this file — nothing is
written to the host computer. To move to a new machine, just copy this
whole folder to a new USB drive or computer.

To stop FreEco.ai, close the terminal/console window it's running in
(or press Ctrl+C).
