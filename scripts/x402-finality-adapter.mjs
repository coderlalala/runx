#!/usr/bin/env node

import { runPaymentFinalityAdapter } from "./lib/payment-finality-adapter.mjs";

const TX_HASH = /^0x[0-9a-fA-F]{64}$/;

runPaymentFinalityAdapter({
  label: "x402 finality adapter",
  rail: "x402",
  proofLocatorFields: ["tx_hash"],
  proofRefProviderLocator: (proofRef) => (TX_HASH.test(proofRef) ? proofRef : undefined),
});
