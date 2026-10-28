"""Prints the exact JSON 1Click signed for a quote response read from stdin.

1Click's SDK signs an allowlist of request and quote fields plus the timestamp, with sorted keys:
Peerfolio Backend's build_signed_payload. A falsy optional field is left out, as the SDK's
JSON.stringify drops undefined. swap takes this string byte for byte.
"""
import json, sys
r = json.load(sys.stdin)
req, q = r["quoteRequest"], r["quote"]
fields = lambda src, keys: {k: src.get(k) for k in keys}
signed = fields(req, ["dry", "swapType", "slippageTolerance", "originAsset", "depositType", "destinationAsset",
    "amount", "refundTo", "refundType", "recipient", "recipientType", "deadline"])
signed |= fields(q, ["amountIn", "amountInFormatted", "amountInUsd", "minAmountIn", "amountOut",
    "amountOutFormatted", "amountOutUsd", "minAmountOut"])
optional = fields(req, ["quoteWaitingTimeMs", "referral", "virtualChainRecipient", "virtualChainRefundRecipient",
    "customRecipientMsg"])
if not req["dry"]:
    optional |= fields(q, ["depositAddress", "depositMemo", "deadline", "timeWhenInactive", "timeEstimate",
        "virtualChainRecipient", "virtualChainRefundRecipient", "customRecipientMsg", "refundFee", "withdrawFee"])
signed |= {k: v for k, v in optional.items() if v}
signed["timestamp"] = r["timestamp"]
print(json.dumps(signed, sort_keys=True, separators=(",", ":"), ensure_ascii=False))
