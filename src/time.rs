// Time management is integrated directly into search::SearchState (check_time)
// and uci::GoParams (compute_time_ms). No separate module needed.
//
// Design:
// - Hard limit: search aborts when elapsed >= time_limit_ms (checked every 2048 nodes)
// - Soft limit: iterative deepening stops if >50% of allocated time used
// - Allocation: my_time / moves_to_go + 3/4 * increment, capped at 80% of remaining time

