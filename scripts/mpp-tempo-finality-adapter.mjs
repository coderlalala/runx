#!/usr/bin/env node

import { runPaymentFinalityAdapter } from "./lib/payment-finality-adapter.mjs";

const TX_HASH = /^0x[0-9a-fA-F]{64}$/;

runPaymentFinalityAdapter({
  label: "mpp-tempo finality adapter",
  rail: "mpp-tempo",
  proofLocatorFields: ["tx_hash"],
  proofRefProviderLocator: (proofRef) => (TX_HASH.test(proofRef) ? proofRef : undefined),
});
