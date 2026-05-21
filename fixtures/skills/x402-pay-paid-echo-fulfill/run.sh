#!/bin/sh
cat <<'JSON'
{"payment_rail_packet":{"data":{"rail_result":{"status":"fulfilled","rail":"mock","amount_minor":125,"currency":"USD"},"credential_envelope":{"form":"paid_tool_credential","credential_ref":"credential:mock:paid-echo-001"},"redactions":["rail_session_material"],"recovery_hint":{"status":"sealed"},"rail_proof":{"proof_ref":"receipt-proof:mock:paid-echo-001","idempotency_key":"payment:paid-echo-001","rail_session_material_ref":"rail-session-material:mock:paid-echo-001"}}}}
JSON
