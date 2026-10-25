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
enum class Case { no_guard, unrestored, hung, capability };
enum class Phase { baseline, acquired, restored };
inline std::optional<Case> parse_case(std::string_view value) {
    if (value == "no-guard") return Case::no_guard;
    if (value == "known-unrestored") return Case::unrestored;
    if (value == "hung-child") return Case::hung;
    if (value == "capability") return Case::capability;
    return std::nullopt;
}
inline const char* case_name(Case value) {
    switch (value) { case Case::no_guard: return "no-guard"; case Case::unrestored: return "known-unrestored"; case Case::hung: return "hung-child"; case Case::capability: return "capability"; }
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
inline std::vector<std::string_view> words(std::string_view line) {
    std::vector<std::string_view> result;
    if (line.size() > line_limit || line.find_first_of("\r\n\t") != std::string_view::npos) return result;
    while (!line.empty()) {
        const auto index = line.find(' ');
        const auto item = line.substr(0, index);
        if (item.empty()) return {};
        result.push_back(item);
        if (result.size() > 16) return {};
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
}
