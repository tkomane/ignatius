#include "protocol.hpp"
#include <iostream>
#include <limits>
#include <stdexcept>
unsigned passed = 0;
void check(bool good, const char* message) { if (!good) throw std::runtime_error(message); ++passed; }
int main() {
    using namespace w07;
    Observation base{Phase::baseline, true, true, true, true, 135, 7, true, true, 0};
    auto after = base; after.phase = Phase::restored;
    check(baseline_valid(base), "positive cooked baseline");
    check(restored(base, after), "positive restoration");
    auto broken = after; broken.input = 152;
    check(!restored(base, broken), "known unrestored native mask must be detected");
    check(!hung_proven(true, true, true, false, true), "termination without owned wait must not count as hung proof");
    check(!hung_proven(false, true, true, true, true), "cold startup must not count as acknowledged hang");
    check(hung_proven(true, true, true, true, true), "positive owned termination control");
    check(parse_observation("O nonce baseline 1 1 1 1 135 7 1 1 0", "nonce").has_value(), "valid bounded observation");
    check(!parse_observation("O other baseline 1 1 1 1 135 7 1 1 0", "nonce"), "nonce required");
    check(!parse_observation("O nonce baseline 2 1 1 1 135 7 1 1 0", "nonce"), "boolean range");
    check(!parse_observation("O nonce baseline 1 1 1 1 4294967296 7 1 1 0", "nonce"), "native mask overflow");
    check(!parse_observation("O nonce baseline 1 1 1 1 -1 7 1 1 0", "nonce"), "negative native mask");
    check(!parse_observation(std::string(4097, 'x'), "nonce"), "line bound");
    Budget bytes{64, 0}; check(bytes.add(64), "exact byte budget"); check(!bytes.add(1), "one over budget"); check(bytes.used == 64, "no wrapped or partial debit");
    check(!Budget{64, 0}.add(std::numeric_limits<std::uint64_t>::max()), "checked size overflow");
    check(required_phase_order(Case::capability, {base, Observation{Phase::acquired}, after}), "ordered phases");
    check(!required_phase_order(Case::capability, {base, after}), "missing acquisition");
    check(!required_phase_order(Case::capability, {base, Observation{Phase::acquired}, after, after}), "duplicate phase");
    check(pipe_read_failure(false, 0, 5) == 5, "unexpected zero-byte failed read must remain an error");
    check(pipe_read_failure(false, 0, 109) == 0, "broken pipe is normal EOF");
    check(pipe_read_failure(true, 0, 5) == 0, "successful zero-byte EOF ignores stale error");
    check(pipe_read_failure(true, 4, 5) == 0, "successful data ignores stale error");
    check(pipe_read_failure(false, 0, 0) == 30, "failed read without native error needs explicit fallback");
    check(pipe_read_failure(false, 4, 5) == 5, "failed partial read cannot claim complete data");
    check(pipe_read_failure(false, 4, 109) == 109, "broken-pipe partial failure is not complete EOF");
    std::cout << passed << " pure protocol/ownership-state checks passed; no Windows calls\n";
}
