#!/usr/bin/env node

import { runPaymentFinalityAdapter } from "./lib/payment-finality-adapter.mjs";

runPaymentFinalityAdapter({
  label: "stripe-spt finality adapter",
  rail: "stripe-spt",
  proofLocatorFields: ["charge_id", "payment_intent_id"],
});
