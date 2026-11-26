-------------------------- MODULE SwapDeliveryNightly -------------------------
EXTENDS Naturals, TLC
CONSTANTS Effects, MaxAttempts
VARIABLES enqueued, pending, sent, acked, failed, alerted, attempts
INSTANCE SwapDelivery WITH
    Keys <- {"k1", "k2", "k3", "k4"},
    Key <- [e \in Effects |->
        CASE e = "e1" -> "k1"
          [] e = "e2" -> "k2"
          [] e = "e3" -> "k3"
          [] OTHER -> "k4"]
=============================================================================
