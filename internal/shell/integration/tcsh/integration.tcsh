# jamalterm shell integration for tcsh / csh (see zsh/integration.zsh).
# postcmd only reports once the first prompt is up, so commands run while
# the startup files load are not mistaken for typed commands.
if ( ! $?_JT_LOADED ) then
  set _JT_LOADED = 1
  alias precmd 'printf "\033]133;D;%s\007\033]7;file://%s%s\007\033]133;A\007" "$status" "$HOST" "$cwd"; set _jt_ready = 1'
  alias postcmd 'if ( $?_jt_ready ) printf "\033]6973;cmd;%s\007\033]133;C\007" "`history -h 1 | base64 | tr -d '"'\n'"'`"'
endif
