// Test-only seam shared by BlockCard.test and BlockCustomerSplit.test. Bun's
// mock.module is process-wide, so whichever file registers @/app/actions
// first wins for both; each file's mock routes fetchBillingRegistry through
// this holder, so the result is the same in either file order.
const defaultRegistry = async () => ({
  ok: true as const,
  data: { customers: [], folders: [], unmapped: [], deildir: [] },
});

export const registryHook = { fetch: defaultRegistry as () => Promise<unknown> };
export const resetRegistryHook = () => {
  registryHook.fetch = defaultRegistry;
};
