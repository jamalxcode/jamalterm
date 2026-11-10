# jamalterm: browse mode on a server, for this session only. jamalterm
# types one line that runs this in the remote bash or zsh; nothing is
# written to the server. Same marks as the local integration scripts.
if [ -n "${_JT_LOADED-}" ]; then
  :
elif [ -n "${ZSH_VERSION-}" ]; then
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
  typeset -ga precmd_functions preexec_functions
  precmd_functions=(_jt_precmd ${precmd_functions:#_jt_precmd})
  preexec_functions=(_jt_preexec ${preexec_functions:#_jt_preexec})
  builtin printf 'jamalterm: browse mode is on for this server session.\n'
elif [ -n "${BASH_VERSION-}" ]; then
  _JT_LOADED=1
  _jt_running=0
  _jt_at_prompt=0
  _jt_precmd() {
    local ec=$?
    if [[ $_jt_running == 1 ]]; then
      builtin printf '\033]133;D;%s\007' "$ec"
      _jt_running=0
    fi
    builtin printf '\033]7;file://%s%s\007' "${HOSTNAME-}" "$PWD"
    builtin printf '\033]133;A\007'
    return $ec
  }
  _jt_ready() { local ec=$?; _jt_at_prompt=1; return $ec; }
  _jt_preexec() {
    [[ $_jt_at_prompt == 1 ]] || return 0
    [[ -n "${COMP_LINE-}" ]] && return 0
    case "$BASH_COMMAND" in _jt_precmd*|_jt_ready*) return 0 ;; esac
    _jt_at_prompt=0
    _jt_running=1
    local cmd
    cmd=$(HISTTIMEFORMAT= builtin history 1 | command sed -e 's/^ *[0-9][0-9]*[* ] *//')
    builtin printf '\033]6973;cmd;%s\007' "$(builtin printf '%s' "$cmd" | command base64 | command tr -d '\n')"
    builtin printf '\033]133;C\007'
  }
  if [[ -n "${bash_preexec_imported-}" ]]; then
    precmd_functions=(_jt_precmd "${precmd_functions[@]}" _jt_ready)
    preexec_functions+=(_jt_preexec)
  else
    # Our hook first, _jt_ready last: other PROMPT_COMMAND entries aren't
    # commands. A DEBUG trap already set keeps running, before ours.
    if [[ "$(declare -p PROMPT_COMMAND 2>/dev/null)" == "declare -a"* ]]; then
      if (( BASH_VERSINFO[0] > 5 || (BASH_VERSINFO[0] == 5 && BASH_VERSINFO[1] >= 1) )); then
        PROMPT_COMMAND=(_jt_precmd "${PROMPT_COMMAND[@]}" _jt_ready)
      else
        _jt_pc=$(IFS=';'; builtin echo "${PROMPT_COMMAND[*]}"); unset PROMPT_COMMAND
        PROMPT_COMMAND="_jt_precmd; ${_jt_pc}; _jt_ready"; unset _jt_pc
      fi
    else
      PROMPT_COMMAND="_jt_precmd${PROMPT_COMMAND:+; $PROMPT_COMMAND}; _jt_ready"
    fi
    _jt_cap() { _jt_prev_trap=$2; }
    _jt_prev_trap=; _jt_t=$(trap -p DEBUG)
    [[ -n "$_jt_t" ]] && eval "_jt_cap ${_jt_t#trap }"
    if [[ -n "$_jt_prev_trap" ]]; then trap "$_jt_prev_trap"$'\n''_jt_preexec' DEBUG; else trap '_jt_preexec' DEBUG; fi
    unset -f _jt_cap; unset _jt_t
  fi
  builtin printf 'jamalterm: browse mode is on for this server session.\n'
else
  printf 'jamalterm: browse mode on a server needs bash or zsh there.\n'
fi
