#pragma once
// Test-only fixed observation arithmetic. No platform calls or product code.
#include <algorithm>
#include <array>
#include <charconv>
#include <cstdint>
#include <optional>
#include <string>
#include <string_view>
#include <vector>
namespace w07 {
constexpr std::uint64_t output_limit = 128 * 1024;
constexpr std::uint64_t observation_limit = 64 * 1024;
constexpr std::uint64_t experiment_limit = 32 * 1024 * 1024;
constexpr std::uint32_t cooked_bits = 0x0007;
constexpr std::uint32_t mouse_bit = 0x0010;
constexpr std::uint32_t vt_output_bit = 0x0004;
constexpr std::size_t line_limit = 4096;
enum class Case { no_guard, unrestored, hung, capability, cursor_routing };
enum class Phase { baseline, acquired, restored };
inline std::optional<Case> parse_case(std::string_view value) {
    if (value == "no-guard") return Case::no_guard;
    if (value == "known-unrestored") return Case::unrestored;
    if (value == "hung-child") return Case::hung;
    if (value == "capability") return Case::capability;
    if (value == "cursor-routing") return Case::cursor_routing;
    return std::nullopt;
}
inline const char* case_name(Case value) {
    switch (value) { case Case::no_guard: return "no-guard"; case Case::unrestored: return "known-unrestored"; case Case::hung: return "hung-child"; case Case::capability: return "capability"; case Case::cursor_routing: return "cursor-routing"; }
    return "invalid";
}
struct Observation {
    Phase phase{};
    bool input_ok{}, output_ok{}, cursor_ok{}, buffer_ok{};
    std::uint32_t input{}, output{};
    bool cursor_visible{}, sentinel_seen{};
    std::uint32_t native_error{};
};
inline bool raw(const Observation& value) { return (value.input & cooked_bits) == 0; }
inline bool baseline_valid(const Observation& value) {
    return value.input_ok && value.output_ok && value.cursor_ok
        && (value.input & cooked_bits) == cooked_bits
        && (value.input & mouse_bit) == 0 && (value.output & vt_output_bit) != 0
        && value.cursor_visible;
}
inline bool restored(const Observation& before, const Observation& after) {
    return baseline_valid(before) && after.input_ok && after.output_ok && after.cursor_ok
        && before.input == after.input && before.output == after.output
        && before.cursor_visible == after.cursor_visible;
}
inline bool hung_proven(bool acknowledged_ready, bool deadline_elapsed, bool terminated_owned, bool waited_owned, bool cleanup_confirmed) {
    return acknowledged_ready && deadline_elapsed && terminated_owned && waited_owned && cleanup_confirmed;
}
// Win32 constants are kept in this platform-independent seam: broken pipe109,
// read fault30. A failed partial read cannot establish complete drainage.
inline std::uint32_t pipe_read_failure(bool succeeded, std::uint32_t bytes, std::uint32_t error) {
    if (succeeded || (bytes == 0 && error == 109)) return 0;
    return error == 0 ? 30 : error;
}
struct Budget {
    std::uint64_t limit{}, used{};
    bool add(std::uint64_t bytes) {
        if (bytes > limit || used > limit - bytes) return false;
        used += bytes; return true;
    }
};
inline bool number(std::string_view text, std::uint32_t& value) {
    if (text.empty()) return false;
    const auto parsed = std::from_chars(text.data(), text.data() + text.size(), value);
    return parsed.ec == std::errc{} && parsed.ptr == text.data() + text.size();
}
inline std::vector<std::string_view> words(std::string_view line, std::size_t bound = 16) {
    std::vector<std::string_view> result;
    if (line.size() > line_limit || line.find_first_of("\r\n\t") != std::string_view::npos) return result;
    while (!line.empty()) {
        const auto index = line.find(' ');
        const auto item = line.substr(0, index);
        if (item.empty()) return {};
        result.push_back(item);
        if (result.size() > bound) return {};
        if (index == std::string_view::npos) break;
        line.remove_prefix(index + 1);
    }
    return result;
}
inline std::optional<Observation> parse_observation(std::string_view line, std::string_view nonce) {
    const auto parts = words(line);
    if (parts.size() != 12 || parts[0] != "O" || parts[1] != nonce) return std::nullopt;
    Observation value;
    if (parts[2] == "baseline") value.phase = Phase::baseline;
    else if (parts[2] == "acquired") value.phase = Phase::acquired;
    else if (parts[2] == "restored") value.phase = Phase::restored;
    else return std::nullopt;
    std::array<std::uint32_t, 9> n{};
    for (std::size_t i = 0; i < n.size(); ++i) if (!number(parts[i + 3], n[i])) return std::nullopt;
    for (auto index : {0U, 1U, 2U, 3U, 6U, 7U}) if (n[index] > 1) return std::nullopt;
    value.input_ok = n[0] != 0; value.output_ok = n[1] != 0;
    value.cursor_ok = n[2] != 0; value.buffer_ok = n[3] != 0;
    value.input = n[4]; value.output = n[5]; value.cursor_visible = n[6] != 0;
    value.sentinel_seen = n[7] != 0; value.native_error = n[8]; return value;
}
inline bool required_phase_order(Case kind, const std::vector<Observation>& values) {
    if (kind == Case::hung) return values.size() == 1 && values[0].phase == Phase::baseline;
    if (kind == Case::unrestored) return values.size() == 2 && values[0].phase == Phase::baseline && values[1].phase == Phase::restored;
    return values.size() == 3 && values[0].phase == Phase::baseline && values[1].phase == Phase::acquired && values[2].phase == Phase::restored;
}
// Cursor calibration is a separate fixed nine-stage diagnostic. It never
// changes the legacy three-phase shapes or accepts product capability.
constexpr std::size_t calibration_stage_count = 9;
inline const char* calibration_stage_name(std::size_t index) {
    constexpr std::array<const char*, 9> names{"baseline-main", "native-hidden", "native-shown", "main-vt-hidden", "main-vt-shown", "alternate-entered", "alternate-vt-hidden", "alternate-vt-shown", "restored-main"};
    return index < names.size() ? names[index] : "invalid";
}
inline std::string calibration_marker(std::string_view nonce, std::size_t index) { return "W07C-" + std::string(nonce) + "-" + std::to_string(index); }
inline std::string calibration_payload(std::string_view nonce, std::size_t index) {
    constexpr std::array<const char*, 9> sequence{"", "", "", "\x1b[?25l", "\x1b[?25h", "\x1b[?1049h", "\x1b[?25l", "\x1b[?25h", "\x1b[?1049l"};
    return index < sequence.size() ? std::string(sequence[index]) + "\x1b[2;1H" + calibration_marker(nonce, index) : "";
}
struct ApiResult { bool ok{}; std::uint32_t error{}; };
inline bool api_ok(const ApiResult& api) { return api.ok && api.error == 0; }
struct CalibrationInit {
    std::array<ApiResult, 7> api{}; // get input/output/cursor; set input/output/cursor; sentinel write
    std::uint32_t written{}, expected{}, input{}, output{}, size{};
    std::uint64_t observed{};
};
struct CalibrationStage {
    std::uint32_t sequence{}, actor{};
    std::array<ApiResult, 4> query{}; // input/output/cursor/sentinel
    bool setter_attempted{}; ApiResult setter{}, write{}, fence{};
    std::uint32_t written{}, expected{}, input{}, output{}, size{}, sentinel_expected{}, sentinel_read{};
    bool visible{}, sentinel_seen{}, marker_seen{};
    std::uint64_t observed{}, acknowledged{};
};
struct CalibrationRestore {
    std::array<ApiResult, 3> api{}, verification{};
    std::uint32_t input{}, output{}, size{};
    bool visible{}, baseline_restored{};
    std::uint64_t verified{}, observed{};
};
struct CalibrationContext {
    std::uint32_t actor{}, registrations{}, attachments{};
    bool cleanup{}; std::uint64_t output_seen{}, output_kept{}, control_bytes{};
};
inline bool calibration_complete(const CalibrationInit& init, const std::vector<CalibrationStage>& stages, const CalibrationRestore& restore, const CalibrationContext& context, std::string_view nonce) {
    if (stages.size()!=calibration_stage_count || context.actor==0 || context.registrations!=1 || context.attachments!=1 || !context.cleanup) return false;
    if (context.output_seen!=context.output_kept || context.output_seen>output_limit || context.control_bytes>observation_limit) return false;
    for(const auto& api:init.api) if(!api_ok(api)) return false;
    if(init.expected!=4+nonce.size() || init.written!=init.expected || init.size<1 || init.size>100 || (init.input&cooked_bits)!=cooked_bits || (init.input&mouse_bit)!=0 || (init.output&vt_output_bit)==0) return false;
    std::uint64_t previous=init.observed, previous_ack=0;
    for(std::size_t i=0;i<stages.size();++i) {
        const auto& s=stages[i]; if(s.sequence!=i || s.actor!=context.actor) return false;
        for(const auto& api:s.query) if(!api_ok(api)) return false;
        const bool setter=i==1 || i==2 || i==8;
        if(s.setter_attempted!=setter || (setter ? !api_ok(s.setter) : (s.setter.ok || s.setter.error!=0))) return false;
        if(!api_ok(s.write) || !api_ok(s.fence) || !s.marker_seen || s.written!=s.expected || s.expected!=calibration_payload(nonce,i).size()) return false;
        if(s.input!=init.input || s.output!=init.output || s.size<1 || s.size>100 || ((i==1||i==2)&&s.size!=init.size)) return false;
        if(s.sentinel_expected!=4+nonce.size() || s.sentinel_read!=s.sentinel_expected) return false;
        if(s.sentinel_seen!=(i<5||i==8) || s.observed<previous || s.acknowledged<s.observed || s.acknowledged<previous_ack) return false;
        previous=s.observed;previous_ack=s.acknowledged;
    }
    for(const auto& api:restore.api)if(!api_ok(api))return false;
    for(const auto& api:restore.verification)if(!api_ok(api))return false;
    if(restore.input!=init.input || restore.output!=init.output || restore.size!=init.size || !restore.visible
        || restore.verified<stages[7].observed || restore.verified>stages[8].observed) return false;
    return stages[0].size==init.size && stages[0].visible && stages[8].visible==stages[0].visible && stages[8].size==stages[0].size && restore.baseline_restored && restore.observed>=previous;
}
inline bool calibration_transition(const std::vector<CalibrationStage>& stages, std::size_t hidden, std::size_t shown) {
    return stages.size() == calibration_stage_count && api_ok(stages[hidden].query[2]) && api_ok(stages[shown].query[2]) && !stages[hidden].visible && stages[shown].visible;
}
inline std::optional<CalibrationStage> parse_calibration_stage(std::string_view line, std::string_view nonce, std::uint32_t actor) {
    const auto fields = words(line, 32);
    if (fields.size() != 30 || fields[0] != "D" || fields[1] != nonce) return std::nullopt;
    std::array<std::uint32_t, 28> n{}; for (std::size_t i=0; i<n.size(); ++i) if (!number(fields[i+2], n[i])) return std::nullopt;
    for (auto i : {1U,3U,5U,7U,9U,10U,12U,16U,22U,23U}) if (n[i] > 1) return std::nullopt;
    CalibrationStage s; s.sequence=n[0]; s.actor=actor;
    for (std::size_t i=0; i<4; ++i) s.query[i]={n[1+i*2]!=0,n[2+i*2]};
    s.setter_attempted=n[9]!=0; s.setter={n[10]!=0,n[11]}; s.write={n[12]!=0,n[13]}; s.written=n[14]; s.expected=n[15]; s.fence={n[16]!=0,n[17]};
    s.input=n[18]; s.output=n[19]; s.size=n[20]; s.visible=n[21]!=0; s.sentinel_seen=n[22]!=0; s.marker_seen=n[23]!=0;
    if (n[21]>1 || s.sequence>=calibration_stage_count) return std::nullopt;
    s.observed=(static_cast<std::uint64_t>(n[25])<<32)|n[24]; s.sentinel_expected=n[26]; s.sentinel_read=n[27]; return s;
}

inline std::optional<CalibrationInit> parse_calibration_init(std::string_view line,std::string_view nonce) {
    const auto f=words(line,32);if(f.size()!=23||f[0]!="I"||f[1]!=nonce)return std::nullopt;
    std::array<std::uint32_t,21> n{};for(std::size_t i=0;i<n.size();++i)if(!number(f[i+2],n[i]))return std::nullopt;
    CalibrationInit r;for(std::size_t i=0;i<7;++i){if(n[i*2]>1)return std::nullopt;r.api[i]={n[i*2]!=0,n[i*2+1]};}
    r.written=n[14];r.expected=n[15];r.input=n[16];r.output=n[17];r.size=n[18];r.observed=(static_cast<std::uint64_t>(n[20])<<32)|n[19];return r;
}
inline std::optional<CalibrationRestore> parse_calibration_restore(std::string_view line,std::string_view nonce) {
    const auto f=words(line,32);if(f.size()!=23||f[0]!="T"||f[1]!=nonce)return std::nullopt;
    std::array<std::uint32_t,21> n{};for(std::size_t i=0;i<n.size();++i)if(!number(f[i+2],n[i]))return std::nullopt;
    CalibrationRestore r;
    for(std::size_t i=0;i<3;++i){
        if(n[i*2]>1||n[6+i*2]>1)return std::nullopt;
        r.api[i]={n[i*2]!=0,n[i*2+1]};r.verification[i]={n[6+i*2]!=0,n[7+i*2]};
    }
    if(n[15]>1||n[16]>1)return std::nullopt;
    r.input=n[12];r.output=n[13];r.size=n[14];r.visible=n[15]!=0;r.baseline_restored=n[16]!=0;
    r.verified=(static_cast<std::uint64_t>(n[18])<<32)|n[17];r.observed=(static_cast<std::uint64_t>(n[20])<<32)|n[19];return r;
}

}
