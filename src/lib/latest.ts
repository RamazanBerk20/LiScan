/** Numbers async requests so only the newest one's reply is applied. */
export function createLatest() {
  let current = 0;
  return {
    next: () => ++current,
    isCurrent: (ticket: number) => ticket === current
  };
}
