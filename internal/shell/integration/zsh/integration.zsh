# jamalterm shell integration for zsh.
# Marks each prompt, command and result with invisible escape codes so the
# terminal knows where every command starts and ends (OSC 133, like iTerm2,
# VS Code and Ghostty), plus the command text and the working directory.
[[ -n "${_JT_LOADED-}" ]] && return
typeset -g _JT_LOADED=1
typeset -gi _jt_running=0

_jt_precmd() {
  local ec=$?
  if (( _jt_running )); then
    builtin printf '\033]133;D;%s\007' "$ec"
    _jt_running=0
  fi
  builtin printf '\033]7;file://%s%s\007' "${HOST-}" "$PWD"
  builtin printf '\033]133;A\007'
}

_jt_preexec() {
  _jt_running=1
  builtin printf '\033]6973;cmd;%s\007' "$(builtin printf '%s' "$1" | command base64 | command tr -d '\n')"
  builtin printf '\033]133;C\007'
}

# Ours runs first so it sees the real exit code.
typeset -ga precmd_functions preexec_functions
precmd_functions=(_jt_precmd ${precmd_functions:#_jt_precmd})
preexec_functions=(_jt_preexec ${preexec_functions:#_jt_preexec})
