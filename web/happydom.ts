// Registers a minimal DOM (via happy-dom) for `bun test`, which has no
// browser globals by default. Only `SettingsPanel.test.tsx` needs this —
// every other web test exercises pure logic and is unaffected — but the
// registration itself is process-global, so it lives in one preload file
// per Bun's own testing-library guide rather than per-test setup.
import { GlobalRegistrator } from "@happy-dom/global-registrator";

GlobalRegistrator.register();
