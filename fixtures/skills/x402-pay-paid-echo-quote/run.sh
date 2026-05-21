#!/bin/sh
cat <<'JSON'
{"payment_quote_packet":{"data":{"payment_signal":{"signal_type":"payment_required","challenge_id":"ch_mock_paid_echo_001","amount_minor":125,"currency":"USD","rail":"mock","counterparty":"merchant:paid-echo","operation":"paid.echo"},"payment_quote":{"quote_id":"quote_paid_echo_001","amount_minor":125,"currency":"USD","rails":["mock"],"counterparty":"merchant:paid-echo","operation":"paid.echo"}}}}
JSON
