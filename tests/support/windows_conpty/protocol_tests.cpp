#include "protocol.hpp"
#include <iostream>
#include <limits>
#include <stdexcept>
unsigned passed = 0;
void check(bool good, const char* message) { if (!good) throw std::runtime_error(message); ++passed; }
struct CalibrationFixture {
    w07::CalibrationInit init; std::vector<w07::CalibrationStage> stages;
    w07::CalibrationRestore restore; w07::CalibrationContext context{42,1,1,true,100,100,1000};
    CalibrationFixture() {
        for (auto& api : init.api) api={true,0}; init.written=9; init.expected=9; init.input=423; init.output=7; init.size=25; init.observed=10;
        for (std::uint32_t i=0;i<9;++i) { w07::CalibrationStage s; s.sequence=i; s.actor=42; for(auto& q:s.query) q={true,0}; s.setter_attempted=i==1||i==2||i==8; s.setter={s.setter_attempted,0}; s.write={true,0}; s.fence={true,0}; s.expected=static_cast<std::uint32_t>(w07::calibration_payload("nonce",i).size()); s.written=s.expected; s.input=423;s.output=7;s.size=25;s.visible=i!=1&&i!=3&&i!=6;s.sentinel_expected=9;s.sentinel_read=9;s.sentinel_seen=i<5||i==8;s.marker_seen=true;s.observed=20+i*2;s.acknowledged=s.observed+1;stages.push_back(s); }
        for(auto& api:restore.api)api={true,0};for(auto& api:restore.verification)api={true,0};restore.input=423;restore.output=7;restore.size=25;restore.visible=true;restore.baseline_restored=true;restore.verified=35;restore.observed=40;
    }
};
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
    CalibrationFixture calibration;
    check(calibration_complete(calibration.init,calibration.stages,calibration.restore,calibration.context,"nonce"),"complete diagnostic accepts separately from product");
    auto bad_calibration=calibration;bad_calibration.stages[3].actor=43;
    check(!calibration_complete(bad_calibration.init,bad_calibration.stages,bad_calibration.restore,bad_calibration.context,"nonce"),"wrong persistent actor cannot complete diagnostic");
    bad_calibration=calibration;bad_calibration.stages[8].visible=false;
    check(!calibration_complete(bad_calibration.init,bad_calibration.stages,bad_calibration.restore,bad_calibration.context,"nonce"),"incomplete restored cursor cannot complete diagnostic");
    bad_calibration=calibration;bad_calibration.context.cleanup=false;
    check(!calibration_complete(bad_calibration.init,bad_calibration.stages,bad_calibration.restore,bad_calibration.context,"nonce"),"unconfirmed cleanup cannot complete diagnostic");
    bad_calibration=calibration;bad_calibration.stages[4].sequence=3;
    check(!calibration_complete(bad_calibration.init,bad_calibration.stages,bad_calibration.restore,bad_calibration.context,"nonce"),"duplicate phase cannot complete");
    bad_calibration=calibration;bad_calibration.stages[6].marker_seen=false;
    check(!calibration_complete(bad_calibration.init,bad_calibration.stages,bad_calibration.restore,bad_calibration.context,"nonce"),"missing parser marker cannot complete");
    bad_calibration=calibration;bad_calibration.stages[6].written--;
    check(!calibration_complete(bad_calibration.init,bad_calibration.stages,bad_calibration.restore,bad_calibration.context,"nonce"),"partial VT write cannot complete");
    bad_calibration=calibration;bad_calibration.restore.api[1]={false,5};
    check(!calibration_complete(bad_calibration.init,bad_calibration.stages,bad_calibration.restore,bad_calibration.context,"nonce"),"restore API failure cannot complete");
    bad_calibration=calibration;bad_calibration.context.registrations=2;
    check(!calibration_complete(bad_calibration.init,bad_calibration.stages,bad_calibration.restore,bad_calibration.context,"nonce"),"second actor registration cannot complete");
    bad_calibration=calibration;bad_calibration.context.control_bytes=observation_limit+1;
    check(!calibration_complete(bad_calibration.init,bad_calibration.stages,bad_calibration.restore,bad_calibration.context,"nonce"),"observation budget overflow cannot complete");
    bad_calibration=calibration;for(auto& s:bad_calibration.stages)s.visible=true;
    check(calibration_complete(bad_calibration.init,bad_calibration.stages,bad_calibration.restore,bad_calibration.context,"nonce"),"constant sensor is completed diagnostic, not capability");
    check(!calibration_transition(bad_calibration.stages,6,7),"constant alternate sensor must report no transition");
    check(calibration_transition(calibration.stages,1,2)&&calibration_transition(calibration.stages,3,4)&&calibration_transition(calibration.stages,6,7),"three independently observed transitions");
    check(words("1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17").empty(),"legacy token bound remains16");
    check(words(std::string(100,'x'),32).size()==1,"diagnostic parser remains bounded");
    const auto d="D nonce 0 1 0 1 0 1 0 1 0 0 0 0 1 0 45 45 1 0 423 7 25 1 1 1 20 0 9 9";
    check(parse_calibration_stage(d,"nonce",42).has_value(),"typed diagnostic stage parses");
    check(!parse_calibration_stage(d,"other",42),"diagnostic nonce required");
    check(!parse_calibration_stage(std::string(d)+" 0","nonce",42),"extra diagnostic field refused");
    check(!parse_calibration_stage("D nonce 9 1 0 1 0 1 0 1 0 0 0 0 1 0 45 45 1 0 423 7 25 1 1 1 20 0 9 9","nonce",42),"out-of-range stage refused");
    check(parse_calibration_init("I nonce 1 0 1 0 1 0 1 0 1 0 1 0 1 0 9 9 423 7 25 10 0","nonce").has_value(),"typed initialization parses");
    check(!parse_calibration_init("I nonce 2 0 1 0 1 0 1 0 1 0 1 0 1 0 9 9 423 7 25 10 0","nonce"),"initialization boolean range");
    check(!parse_calibration_init("I nonce 1 0","nonce"),"incomplete initialization refused");
    check(parse_calibration_restore("T nonce 1 0 1 0 1 0 1 0 1 0 1 0 423 7 25 1 1 35 0 40 0","nonce").has_value(),"typed restoration parses");
    check(!parse_calibration_restore("T nonce 1 0 1 0 1 0 1 0 1 0 1 0 423 7 25 1 2 35 0 40 0","nonce"),"restoration boolean range");
    check(!parse_calibration_restore("T nonce 1 0 1 0 1 0 1 0 1 0 1 0 423 7 25 1 1 35 0 40 0 0","nonce"),"extra restoration field refused");
    bad_calibration=calibration;bad_calibration.init.written=8;bad_calibration.init.expected=8;
    check(!calibration_complete(bad_calibration.init,bad_calibration.stages,bad_calibration.restore,bad_calibration.context,"nonce"),"baseline sentinel count derives from nonce");
    bad_calibration=calibration;bad_calibration.stages[0].size=75;bad_calibration.stages[8].size=75;
    check(!calibration_complete(bad_calibration.init,bad_calibration.stages,bad_calibration.restore,bad_calibration.context,"nonce"),"coherent final cursor size must match seeded baseline");
    bad_calibration=calibration;bad_calibration.stages[6].sentinel_read=8;
    check(!calibration_complete(bad_calibration.init,bad_calibration.stages,bad_calibration.restore,bad_calibration.context,"nonce"),"TRUE sentinel short read cannot prove alternate absence");
    bad_calibration=calibration;bad_calibration.restore.verification[0]={false,5};
    check(!calibration_complete(bad_calibration.init,bad_calibration.stages,bad_calibration.restore,bad_calibration.context,"nonce"),"restoration verification API failure cannot hide behind good stage8");
    bad_calibration=calibration;bad_calibration.stages[6].sentinel_seen=false;
    check(calibration_complete(bad_calibration.init,bad_calibration.stages,bad_calibration.restore,bad_calibration.context,"nonce"),"TRUE full read with different content proves alternate absence");
    for(std::size_t i=0;i<9;++i){bad_calibration=calibration;--bad_calibration.stages[i].sentinel_read;
        check(!calibration_complete(bad_calibration.init,bad_calibration.stages,bad_calibration.restore,bad_calibration.context,"nonce"),"all nine stages refuse TRUE-short sentinel reads");}
    bad_calibration=calibration;bad_calibration.stages[6].sentinel_expected=8;bad_calibration.stages[6].sentinel_read=8;
    check(!calibration_complete(bad_calibration.init,bad_calibration.stages,bad_calibration.restore,bad_calibration.context,"nonce"),"sentinel requested count must equal full nonce sentinel");
    for(std::size_t i=0;i<3;++i){bad_calibration=calibration;bad_calibration.restore.verification[i]={false,5};
        check(!calibration_complete(bad_calibration.init,bad_calibration.stages,bad_calibration.restore,bad_calibration.context,"nonce"),"every restoration verification API failure refuses completion");}
    bad_calibration=calibration;bad_calibration.restore.input=152;
    check(!calibration_complete(bad_calibration.init,bad_calibration.stages,bad_calibration.restore,bad_calibration.context,"nonce"),"independent restoration input mismatch survives good stage8");
    bad_calibration=calibration;bad_calibration.restore.output=3;
    check(!calibration_complete(bad_calibration.init,bad_calibration.stages,bad_calibration.restore,bad_calibration.context,"nonce"),"independent restoration output mismatch survives good stage8");
    bad_calibration=calibration;bad_calibration.restore.size=75;
    check(!calibration_complete(bad_calibration.init,bad_calibration.stages,bad_calibration.restore,bad_calibration.context,"nonce"),"independent restoration cursor size mismatch survives good stage8");
    bad_calibration=calibration;bad_calibration.restore.visible=false;
    check(!calibration_complete(bad_calibration.init,bad_calibration.stages,bad_calibration.restore,bad_calibration.context,"nonce"),"independent restoration visibility mismatch survives good stage8");
    bad_calibration=calibration;bad_calibration.restore.verified=37;
    check(!calibration_complete(bad_calibration.init,bad_calibration.stages,bad_calibration.restore,bad_calibration.context,"nonce"),"verification cannot be substituted by later stage8 query");
    std::cout << passed << " pure protocol/ownership-state checks passed; no Windows calls\n";
}
