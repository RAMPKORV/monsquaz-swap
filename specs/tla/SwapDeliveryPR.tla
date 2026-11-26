---------------------------- MODULE SwapDeliveryPR ----------------------------
EXTENDS Naturals, TLC
CONSTANTS Effects, MaxAttempts
VARIABLES enqueued, pending, sent, acked, attempts
INSTANCE SwapDelivery WITH
    Keys <- {"k1", "k2", "k3"},
    Key <- [e \in Effects |->
        CASE e = "e1" -> "k1"
          [] e = "e2" -> "k2"
          [] OTHER -> "k3"]
=============================================================================
