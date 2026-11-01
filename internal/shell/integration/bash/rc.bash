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
    _jt_at_prompt=1
    return $ec
  }

  _jt_preexec() {
    [[ $_jt_at_prompt == 1 ]] || return 0
    [[ -n "${COMP_LINE-}" ]] && return 0
    case "$BASH_COMMAND" in _jt_precmd*) return 0 ;; esac
    _jt_at_prompt=0
    _jt_running=1
    local cmd
    cmd=$(HISTTIMEFORMAT= builtin history 1 | command sed -e 's/^ *[0-9][0-9]*[* ] *//')
    builtin printf '\033]6973;cmd;%s\007' "$(builtin printf '%s' "$cmd" | command base64 | command tr -d '\n')"
    builtin printf '\033]133;C\007'
  }

  if [[ -n "${bash_preexec_imported-}" ]]; then
    # bash-preexec owns the DEBUG trap: register through it.
    precmd_functions=(_jt_precmd "${precmd_functions[@]}")
    preexec_functions+=(_jt_preexec)
  else
    PROMPT_COMMAND="_jt_precmd${PROMPT_COMMAND:+; $PROMPT_COMMAND}"
    trap '_jt_preexec' DEBUG
  fi
fi
