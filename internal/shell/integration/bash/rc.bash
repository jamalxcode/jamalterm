# jamalterm: bash starts with this file instead of ~/.bashrc. It loads your
# usual startup files first, then adds the hooks.
if [[ -z "${_JT_LOADED-}" ]]; then
  _JT_LOADED=1
  if [[ "${JAMALTERM_BASH_LOGIN-}" == 1 ]]; then
    # Same files a login shell reads (what Terminal.app starts).
    [ -r /etc/profile ] && . /etc/profile
    if [ -r ~/.bash_profile ]; then . ~/.bash_profile
    elif [ -r ~/.bash_login ]; then . ~/.bash_login
    elif [ -r ~/.profile ]; then . ~/.profile
    fi
  else
    [ -r ~/.bashrc ] && . ~/.bashrc
  fi
  unset JAMALTERM_BASH_LOGIN

  # Works with the bash 3.2 that ships with macOS.
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

  # Runs last in PROMPT_COMMAND: only what you type after it is a command.
  # (Prompt tools such as starship add their own PROMPT_COMMAND entries; the
  # DEBUG trap fires for those too, and they must not count as commands.)
  _jt_ready() {
    local ec=$?
    _jt_at_prompt=1
    return $ec
  }

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
    # bash-preexec owns the DEBUG trap: register through it.
    precmd_functions=(_jt_precmd "${precmd_functions[@]}" _jt_ready)
    preexec_functions+=(_jt_preexec)
  else
    if [[ "$(declare -p PROMPT_COMMAND 2>/dev/null)" == "declare -a"* ]]; then
      PROMPT_COMMAND=(_jt_precmd "${PROMPT_COMMAND[@]}" _jt_ready) # bash 5.1+ list
    else
      PROMPT_COMMAND="_jt_precmd${PROMPT_COMMAND:+; $PROMPT_COMMAND}; _jt_ready"
    fi
    # Keep a DEBUG trap you (or starship) already set: it runs first, with
    # its $_ intact, then ours.
    _jt_capture_trap() { _jt_prev_trap=$2; }
    _jt_prev_trap=
    _jt_trap_line=$(trap -p DEBUG)
    [[ -n "$_jt_trap_line" ]] && eval "_jt_capture_trap ${_jt_trap_line#trap }"
    if [[ -n "$_jt_prev_trap" ]]; then
      trap "$_jt_prev_trap"$'\n''_jt_preexec' DEBUG
    else
      trap '_jt_preexec' DEBUG
    fi
    unset -f _jt_capture_trap
    unset _jt_trap_line
  fi
fi
