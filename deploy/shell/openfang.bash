# OpenFang bash completion.
#
# INSTALL (per-user):
#   mkdir -p ~/.local/share/bash-completion/completions
#   cp deploy/shell/openfang.bash ~/.local/share/bash-completion/completions/openfang
#   # Restart your shell or `source` the file.
#
# INSTALL (system-wide, requires sudo):
#   sudo cp deploy/shell/openfang.bash /etc/bash_completion.d/openfang

_openfang_completion() {
    local cur prev cmd
    COMPREPLY=()
    cur="${COMP_WORDS[COMP_CWORD]}"
    prev="${COMP_WORDS[COMP_CWORD-1]}"
    cmd="${COMP_WORDS[1]:-}"

    # Top-level subcommands. Keep this list narrow — it should reflect the
    # subcommands `openfang --help` advertises. Stale completions are worse
    # than missing completions.
    local SUBCOMMANDS="start stop status config agents budget pinboard \
        approvals soul reflection schedules channels skills hands version help"

    if [[ ${COMP_CWORD} -eq 1 ]]; then
        COMPREPLY=( $(compgen -W "${SUBCOMMANDS}" -- "${cur}") )
        return 0
    fi

    case "${cmd}" in
        config)
            COMPREPLY=( $(compgen -W "get set set-key list edit" -- "${cur}") )
            ;;
        agents)
            COMPREPLY=( $(compgen -W "list show spawn stop suspend resume" -- "${cur}") )
            ;;
        budget)
            COMPREPLY=( $(compgen -W "show set agents reset" -- "${cur}") )
            ;;
        pinboard)
            COMPREPLY=( $(compgen -W "list show allow quarantine comment" -- "${cur}") )
            ;;
        soul)
            COMPREPLY=( $(compgen -W "show edit diff approve reject" -- "${cur}") )
            ;;
        reflection)
            COMPREPLY=( $(compgen -W "run show log" -- "${cur}") )
            ;;
        schedules)
            COMPREPLY=( $(compgen -W "list show add remove run" -- "${cur}") )
            ;;
        channels)
            COMPREPLY=( $(compgen -W "list status reload" -- "${cur}") )
            ;;
        skills)
            COMPREPLY=( $(compgen -W "list show install uninstall reload" -- "${cur}") )
            ;;
        hands)
            COMPREPLY=( $(compgen -W "list show install config" -- "${cur}") )
            ;;
    esac
    return 0
}

complete -F _openfang_completion openfang
