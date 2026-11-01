# jamalterm shell integration for tcsh / csh (see zsh/integration.zsh).
# postcmd only reports once the first prompt is up, so commands run while
# the startup files load are not mistaken for typed commands.
# Your own precmd / postcmd aliases keep working: they run after ours.
if ( ! $?_JT_LOADED ) then
  set _JT_LOADED = 1
  set _jt_user_pre = ''
  set _jt_user_post = ''
  if ( "`alias precmd`" != "" ) then
    alias _jt_user_precmd "`alias precmd`"
    set _jt_user_pre = '; _jt_user_precmd'
  endif
  if ( "`alias postcmd`" != "" ) then
    alias _jt_user_postcmd "`alias postcmd`"
    set _jt_user_post = '; _jt_user_postcmd'
  endif
  alias precmd 'printf "\033]133;D;%s\007\033]7;file://%s%s\007\033]133;A\007" "$status" "$HOST" "$cwd"; set _jt_ready = 1'"$_jt_user_pre"
  alias postcmd 'if ( $?_jt_ready ) printf "\033]6973;cmd;%s\007\033]133;C\007" "`history -h 1 | base64 | tr -d '"'\n'"'`"'"$_jt_user_post"
endif
