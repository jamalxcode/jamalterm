# jamalterm shell integration for fish (see zsh/integration.zsh).
# Loaded with `fish -C`, after your own config.fish.
if not set -q _JT_LOADED
    set -g _JT_LOADED 1
    set -g _jt_running 0

    function _jt_prompt --on-event fish_prompt
        set -l ec $status
        if test $_jt_running = 1
            printf '\e]133;D;%s\a' $ec
            set -g _jt_running 0
        end
        printf '\e]7;file://%s%s\a' (hostname) "$PWD"
        printf '\e]133;A\a'
    end

    function _jt_preexec --on-event fish_preexec
        set -g _jt_running 1
        printf '\e]6973;cmd;%s\a' (printf '%s' "$argv" | base64 | tr -d '\n')
        printf '\e]133;C\a'
    end
end
