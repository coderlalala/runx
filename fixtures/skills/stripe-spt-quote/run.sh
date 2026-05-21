#!/bin/sh
cat <<'JSON'
{"payment_quote_packet":{"data":{"payment_signal":{"signal_type":"payment_required","challenge_id":"ch_stripe_spt_001","amount_minor":125,"currency":"USD","rail":"stripe-spt","counterparty":"merchant:stripe-demo","operation":"search.paid"},"payment_quote":{"quote_id":"quote_stripe_spt_001","amount_minor":125,"currency":"USD","rails":["stripe-spt"],"counterparty":"merchant:stripe-demo","operation":"search.paid"}}}}
JSON
