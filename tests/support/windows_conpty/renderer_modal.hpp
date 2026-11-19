#pragma once

// Include before the host's anonymous namespace. Call this from the console
// supervisor, after its exact Job/image/PID/HWND check and before its work
// deadline. The outer controller's process watchdog is the hard wall bound.
#include <windows.h>
#include <oleauto.h>
#include <uiautomation.h>
#include <wrl/client.h>

#include <algorithm>
#include <chrono>
#include <cstdint>
#include <string_view>
#include <utility>

#ifdef _MSC_VER
#pragma comment(lib, "Ole32.lib")
#pragma comment(lib, "OleAut32.lib")
#pragma comment(lib, "UIAutomationCore.lib")
#endif

namespace ignatius_renderer_modal {
using Clock = std::chrono::steady_clock;
using namespace std::chrono_literals;
using Microsoft::WRL::ComPtr;

constexpr UINT text_limit = 2048;
constexpr unsigned node_limit = 96;
constexpr unsigned depth_limit = 12;
constexpr auto provider_timeout = 180ms;

// Microsoft Windows Terminal v1.25.3131.0, TerminalApp en-US Resources.resw.
constexpr std::wstring_view error_title = L"Failed to load settings";
constexpr std::wstring_view error_body =
    L"Settings could not be loaded from file. Check for syntax errors, including trailing commas.";
constexpr std::wstring_view defaults_body = L"Temporarily using the Windows Terminal default settings.";
constexpr std::wstring_view warning_title = L"Encountered errors while loading user settings";

struct JsonLocation {
    unsigned line{}, column{};
};

inline unsigned take_decimal(std::wstring_view& input) noexcept {
    if (input.empty() || input.front() < L'1' || input.front() > L'9') return 0;
    unsigned value = 0;
    for (unsigned digits = 0; !input.empty() && input.front() >= L'0' && input.front() <= L'9'; ++digits) {
        if (digits == 4) return 0;
        value = value * 10 + static_cast<unsigned>(input.front() - L'0');
        input.remove_prefix(1);
    }
    return value <= 4096 ? value : 0;
}

inline bool take(std::wstring_view& input, std::wstring_view prefix) noexcept {
    if (input.substr(0, prefix.size()) != prefix) return false;
    input.remove_prefix(prefix.size());
    return true;
}

inline JsonLocation json_location(std::wstring_view line) noexcept {
    if (!take(line, L"* Line ")) return {};
    const auto row = take_decimal(line);
    if (!row || !take(line, L", Column ")) return {};
    const auto column = take_decimal(line);
    if (!column) return {};
    if (!line.empty()) {
        if (!take(line, L" (") || line.size() < 2 || line.size() > 81 || line.back() != L')') return {};
        line.remove_suffix(1);
        if (line.find_first_not_of(L"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_.-")
            != std::wstring_view::npos) return {};
    }
    return {row, column};
}

inline std::uint32_t labeled_hresult(std::wstring_view line) noexcept {
    if (!take(line, L"HRESULT: ") && !take(line, L"HRESULT ")) return 0;
    if (!take(line, L"0x") || line.size() != 8) return 0;
    std::uint32_t value = 0;
    for (const auto digit : line) {
        unsigned nibble;
        if (digit >= L'0' && digit <= L'9') nibble = static_cast<unsigned>(digit - L'0');
        else if (digit >= L'A' && digit <= L'F') nibble = static_cast<unsigned>(digit - L'A') + 10;
        else if (digit >= L'a' && digit <= L'f') nibble = static_cast<unsigned>(digit - L'a') + 10;
        else return 0;
        value = (value << 4) | nibble;
    }
    return (value & 0x80000000u) ? value : 0;
}

struct ObservedText {
    bool error{}, description{}, fallback{}, warning{};
    JsonLocation json{};
    std::uint32_t reported_hresult{};

    bool confirmed_error() const noexcept { return error && description && fallback; }

    void inspect(std::wstring_view name) noexcept {
        if (name.size() > text_limit) return;
        error |= name == error_title;
        warning |= name == warning_title;
        description |= name.find(error_body) != std::wstring_view::npos;
        fallback |= name.find(defaults_body) != std::wstring_view::npos;
        while (!name.empty()) {
            const auto separator = name.find_first_of(L"\r\n");
            const auto line = name.substr(0, separator);
            if (!json.line) json = json_location(line);
            if (!reported_hresult) reported_hresult = labeled_hresult(line);
            if (separator == std::wstring_view::npos) break;
            name.remove_prefix(separator + 1);
        }
    }
};

struct ModalReceipt {
    const char* category = "not_observed";
    HRESULT automation_hresult = S_OK;
    std::uint32_t settings_hresult{};
    JsonLocation json{};
};

inline ModalReceipt receipt(HRESULT result, const ObservedText& text) noexcept {
    if (text.confirmed_error())
        return {"settings_load_error", result, text.reported_hresult, text.json};
    if (text.error)
        return {"settings_load_title_only", result};
    if (text.warning)
        return {"settings_validation_warning", result};
    return {FAILED(result) ? "uia_incomplete" : "not_observed", result};
}

struct Apartment {
    HRESULT result = CoInitializeEx(nullptr, COINIT_MULTITHREADED);
    Apartment() = default;
    Apartment(const Apartment&) = delete;
    Apartment& operator=(const Apartment&) = delete;
    ~Apartment() { if (SUCCEEDED(result)) CoUninitialize(); }
};

struct Name {
    BSTR value{};
    Name() = default;
    Name(const Name&) = delete;
    Name& operator=(const Name&) = delete;
    ~Name() { SysFreeString(value); }
};

template<typename Request>
HRESULT request(Clock::time_point deadline, Request&& operation) {
    return Clock::now() < deadline ? operation() : HRESULT_FROM_WIN32(ERROR_TIMEOUT);
}

struct Scan {
    IUIAutomationTreeWalker& walker;
    DWORD owned_pid;
    Clock::time_point deadline;
    ObservedText text;
    unsigned visited{};
    bool depth_limited{};

    HRESULT inspect(IUIAutomationElement& parent, unsigned depth) {
        if (depth == depth_limit) {
            depth_limited = true;
            return S_OK;
        }

        ComPtr<IUIAutomationElement> child;
        auto result = request(deadline, [&] { return walker.GetFirstChildElement(&parent, child.GetAddressOf()); });
        while (SUCCEEDED(result) && child) {
            if (++visited > node_limit) return HRESULT_FROM_WIN32(ERROR_MORE_DATA);
            int pid{};
            result = request(deadline, [&] { return child->get_CurrentProcessId(&pid); });
            if (FAILED(result)) return result;
            if (pid > 0 && static_cast<DWORD>(pid) == owned_pid) {
                CONTROLTYPEID type{};
                result = request(deadline, [&] { return child->get_CurrentControlType(&type); });
                if (FAILED(result)) return result;
                if (type == UIA_TextControlTypeId || type == UIA_WindowControlTypeId
                    || type == UIA_PaneControlTypeId || type == UIA_GroupControlTypeId) {
                    BOOL offscreen{};
                    result = request(deadline, [&] { return child->get_CurrentIsOffscreen(&offscreen); });
                    if (FAILED(result)) return result;
                    if (!offscreen) {
                        Name name;
                        result = request(deadline, [&] { return child->get_CurrentName(&name.value); });
                        if (FAILED(result)) return result;
                        if (name.value) {
                            const auto length = SysStringLen(name.value);
                            if (length <= text_limit) text.inspect({name.value, length});
                        }
                        if (text.confirmed_error()) return S_OK;
                    }
                }
                result = inspect(*child.Get(), depth + 1);
                if (FAILED(result) || text.confirmed_error()) return result;
            }
            ComPtr<IUIAutomationElement> sibling;
            result = request(deadline, [&] { return walker.GetNextSiblingElement(child.Get(), sibling.GetAddressOf()); });
            child = std::move(sibling);
        }
        return result;
    }
};

inline ModalReceipt inspect(HWND owned_window, DWORD owned_pid, Clock::time_point work_deadline) {
    const auto start = Clock::now();
    if (start + provider_timeout >= work_deadline)
        return receipt(HRESULT_FROM_WIN32(ERROR_TIMEOUT), {});
    const auto deadline = (std::min)(work_deadline - provider_timeout, start + 1600ms);
    DWORD actual{};
    if (GetWindowThreadProcessId(owned_window, &actual) == 0 || owned_pid == 0 || actual != owned_pid)
        return receipt(HRESULT_FROM_WIN32(ERROR_INVALID_WINDOW_HANDLE), {});

    Apartment apartment;
    if (FAILED(apartment.result)) return receipt(apartment.result, {});
    ComPtr<IUIAutomation2> automation;
    auto result = request(deadline, [&] {
        return CoCreateInstance(__uuidof(CUIAutomation8), nullptr, CLSCTX_INPROC_SERVER,
                                IID_PPV_ARGS(automation.GetAddressOf()));
    });
    if (FAILED(result)) return receipt(result, {});
    constexpr auto timeout_ms = static_cast<DWORD>(provider_timeout.count());
    result = request(deadline, [&] { return automation->put_ConnectionTimeout(timeout_ms); });
    if (FAILED(result)) return receipt(result, {});
    result = request(deadline, [&] { return automation->put_TransactionTimeout(timeout_ms); });
    if (FAILED(result)) return receipt(result, {});

    ComPtr<IUIAutomationElement> root;
    result = request(deadline, [&] { return automation->ElementFromHandle(owned_window, root.GetAddressOf()); });
    if (FAILED(result) || !root) return receipt(FAILED(result) ? result : E_UNEXPECTED, {});
    ComPtr<IUIAutomationTreeWalker> walker;
    result = request(deadline, [&] { return automation->get_ControlViewWalker(walker.GetAddressOf()); });
    if (FAILED(result) || !walker) return receipt(FAILED(result) ? result : E_UNEXPECTED, {});

    Scan scan{*walker.Get(), owned_pid, deadline, {}};
    result = scan.inspect(*root.Get(), 0);
    if (GetWindowThreadProcessId(owned_window, &actual) == 0 || actual != owned_pid)
        return receipt(HRESULT_FROM_WIN32(ERROR_INVALID_WINDOW_HANDLE), {});
    if (SUCCEEDED(result) && !scan.text.confirmed_error() && scan.depth_limited)
        result = HRESULT_FROM_WIN32(ERROR_MORE_DATA);
    return receipt(result, scan.text);
}
} // namespace ignatius_renderer_modal
