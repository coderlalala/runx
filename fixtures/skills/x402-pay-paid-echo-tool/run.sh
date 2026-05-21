#!/bin/sh
inputs=${RUNX_INPUTS_JSON:-}
case "$inputs" in
  *credential_envelope*|*rail_session_material*|*rail-session-material:mock:paid-echo-001*|*stripe_client_secret*|*stripe_api_key*|*stripe_webhook_secret*|*card_number*)
    printf '%s\n' "paid echo received raw payment rail material" >&2
    exit 1
    ;;
esac

if [ "${RUNX_INPUT_PAYMENT_CAPABILITY_REF:-}" != "credential:mock:paid-echo-001" ]; then
  printf '%s\n' "paid echo missing scoped payment capability reference" >&2
  exit 1
fi

if [ "${RUNX_INPUT_PAYMENT_PROOF_REF:-}" != "receipt-proof:mock:paid-echo-001" ]; then
  printf '%s\n' "paid echo missing sealed payment proof reference" >&2
  exit 1
fi

printf '%s' '{"paid_echo_result":{"message":"hello from paid echo","payment_capability_ref":"credential:mock:paid-echo-001","payment_proof_ref":"receipt-proof:mock:paid-echo-001","input_surface":"sealed_refs_only"}}'
