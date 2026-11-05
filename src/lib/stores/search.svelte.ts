// Whether the command bar is open. Ctrl+K toggles it from anywhere in the shell.
export const searchUi = $state({ open: false });

export const openSearch = () => (searchUi.open = true);
export const closeSearch = () => (searchUi.open = false);
