export function mount(root, api) {
  api.commandBus.dispatch({ type: 'x' });
}
