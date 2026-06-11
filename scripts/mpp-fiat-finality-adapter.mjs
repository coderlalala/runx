#!/usr/bin/env node

import { runPaymentFinalityAdapter } from "./lib/payment-finality-adapter.mjs";

runPaymentFinalityAdapter({
  label: "mpp-fiat finality adapter",
  rail: "mpp-fiat",
  proofLocatorFields: ["charge_id", "payment_intent_id"],
});
