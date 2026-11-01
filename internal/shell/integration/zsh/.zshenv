# jamalterm: loaded instead of your ~/.zshenv. Puts ZDOTDIR back so zsh
# reads all of your own startup files as usual, then adds the hooks.
if [[ -n "${JAMALTERM_ZDOTDIR-}" ]]; then
  ZDOTDIR="$JAMALTERM_ZDOTDIR"
else
  unset ZDOTDIR
fi
unset JAMALTERM_ZDOTDIR
[[ -r "${ZDOTDIR:-$HOME}/.zshenv" ]] && builtin source "${ZDOTDIR:-$HOME}/.zshenv"
if [[ -o interactive && -n "${JAMALTERM_INTEGRATION-}" ]]; then
  builtin source "$JAMALTERM_INTEGRATION/zsh/integration.zsh"
fi
