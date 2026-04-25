#compdef openfang
# OpenFang zsh completion.
#
# INSTALL (per-user):
#   mkdir -p ~/.zfunc
#   cp deploy/shell/openfang.zsh ~/.zfunc/_openfang
#   # Add to ~/.zshrc if not already present:
#   #   fpath=(~/.zfunc $fpath)
#   #   autoload -Uz compinit && compinit
#
# INSTALL (system-wide):
#   sudo cp deploy/shell/openfang.zsh /usr/local/share/zsh/site-functions/_openfang
#
# As with the bash counterpart, this list reflects the subcommands openfang
# advertises today. Refresh on every release.

_openfang() {
    local -a subcommands
    subcommands=(
        'start:Start the daemon (foreground)'
        'stop:Send graceful shutdown to a running daemon'
        'status:Print daemon status + uptime'
        'config:Inspect or modify ~/.openfang/config.toml'
        'agents:Manage agents (list/show/spawn/stop/suspend/resume)'
        'budget:Inspect or modify the global budget envelope'
        'pinboard:Review the triage pinboard (Phase 5)'
        'approvals:[deprecated alias of pinboard]'
        'soul:Inspect / approve / reject SOUL.md reflection patches'
        'reflection:Run or inspect the soul-reflection cron'
        'schedules:Manage cron-driven scheduled jobs'
        'channels:Status of channel adapters (Slack/Discord/etc.)'
        'skills:Manage skill registry'
        'hands:Manage bundled hands (collector/researcher/browser/...)'
        'version:Print version'
        'help:Print help'
    )

    _arguments -C \
        '1: :->subcommand' \
        '*::arg:->args'

    case $state in
        subcommand)
            _describe 'subcommand' subcommands
            ;;
        args)
            case $line[1] in
                config)
                    _values 'config action' \
                        'get[Read a config field]' \
                        'set[Write a config field]' \
                        'set-key[Store an API key]' \
                        'list[Print all fields]' \
                        'edit[Open in $EDITOR]'
                    ;;
                agents)
                    _values 'agents action' \
                        'list[List all agents]' \
                        'show[Show one agent]' \
                        'spawn[Spawn from manifest]' \
                        'stop[Stop an agent]' \
                        'suspend[Pause without removing]' \
                        'resume[Resume a suspended agent]'
                    ;;
                budget)
                    _values 'budget action' \
                        'show[Print envelope and spend]' \
                        'set[Update envelope]' \
                        'agents[Per-agent breakdown]' \
                        'reset[Reset accumulated spend]'
                    ;;
                pinboard)
                    _values 'pinboard action' \
                        'list[List entries]' \
                        'show[Show one entry by id]' \
                        'allow[Mark entry allowed]' \
                        'quarantine[Mark entry quarantined]' \
                        'comment[Append an audit-only note]'
                    ;;
                soul)
                    _values 'soul action' \
                        'show[Print current SOUL.md frontmatter]' \
                        'edit[Open in $EDITOR]' \
                        'diff[Show pending soul_patch_proposal.md]' \
                        'approve[Apply a pending proposal]' \
                        'reject[Discard a pending proposal]'
                    ;;
                reflection)
                    _values 'reflection action' \
                        'run[Force a reflection cycle now]' \
                        'show[Print last reflection log entry]' \
                        'log[Print the full cadence log]'
                    ;;
                schedules)
                    _values 'schedules action' \
                        'list[List jobs]' \
                        'show[Show one job]' \
                        'add[Add a job]' \
                        'remove[Remove a job]' \
                        'run[Run a job now]'
                    ;;
                channels)
                    _values 'channels action' \
                        'list[List configured adapters]' \
                        'status[Show connection state]' \
                        'reload[Hot-reload the bridge manager]'
                    ;;
                skills)
                    _values 'skills action' \
                        'list[List installed skills]' \
                        'show[Show one skill]' \
                        'install[Install from path or ClawHub]' \
                        'uninstall[Remove a skill]' \
                        'reload[Hot-reload registry]'
                    ;;
                hands)
                    _values 'hands action' \
                        'list[List bundled hands]' \
                        'show[Show one hand]' \
                        'install[Activate a hand]' \
                        'config[Edit hand config]'
                    ;;
            esac
            ;;
    esac
}

_openfang "$@"
