// Test-only synthetic Windows Terminal calibration. Included in the
// Windows host's anonymous namespace, after renderer_pixels.hpp.
constexpr std::size_t renderer_stage_count = 6;
constexpr std::array<const char*, renderer_stage_count> renderer_stage_names{
    "main-show", "main-hide", "alt-enter-hidden", "alt-show", "alt-hide", "restored-main"
};
constexpr std::array<bool, renderer_stage_count> renderer_visible{true, false, false, true, false, true};
constexpr std::array<bool, renderer_stage_count> renderer_main{true, true, false, false, false, true};
constexpr std::array<const char*, 4> renderer_profile_states{"missing", "zero", "different", "matched"};
constexpr wchar_t renderer_restore_colors[] = L"\x1b]111\x1b\\\x1b]112\x1b\\";

std::wstring renderer_pipe_name(const std::wstring& nonce) {
    return L"\\\\.\\pipe\\ignatius-w07-renderer-" + nonce;
}

std::vector<std::wstring> renderer_environment(const std::filesystem::path& work) {
    // Terminal's portable mode still resolves Windows known folders for
    // application settings fragments. Supply empty task-owned locations.
    const auto profile = work / L"portable-profile";
    const auto local = profile / L"AppData" / L"Local";
    const auto roaming = profile / L"AppData" / L"Roaming";
    const auto shared = work / L"portable-program-data";
    require(std::filesystem::create_directory(profile) && std::filesystem::create_directories(local)
            && std::filesystem::create_directories(roaming) && std::filesystem::create_directory(shared),
            "create fresh portable renderer known folders");
    return {L"ALLUSERSPROFILE=" + shared.wstring(), L"APPDATA=" + roaming.wstring(),
            L"LOCALAPPDATA=" + local.wstring(), L"ProgramData=" + shared.wstring(),
            L"USERPROFILE=" + profile.wstring()};
}

void require_renderer_nonce(const std::wstring& nonce) {
    require(nonce.size() == 32
            && nonce.find_first_not_of(L"0123456789abcdef") == std::wstring::npos,
            "renderer nonce is 32 lower-case hexadecimal characters");
}

std::wstring renderer_tile(int row, int column, int cells, RendererRgb color) {
    return L"\x1b[" + std::to_wstring(row) + L";" + std::to_wstring(column) + L"H\x1b[48;2;"
        + std::to_wstring(static_cast<unsigned>(color.r)) + L";"
        + std::to_wstring(static_cast<unsigned>(color.g)) + L";"
        + std::to_wstring(static_cast<unsigned>(color.b)) + L"m"
        + std::wstring(static_cast<std::size_t>(cells), L' ') + L"\x1b[0m";
}

std::wstring renderer_payload(const std::wstring& nonce, std::size_t phase) {
    std::wstring payload;
    if (phase == 0) {
        payload = L"\x1b[0m\x1b]11;rgb:10/10/10\x1b\\\x1b[2J";
        payload += renderer_tile(4, 2, 6, {125, 32, 220});
        payload += L"\x1b[4;12HW07-" + nonce;
    } else if (phase == 2) {
        payload = L"\x1b[?1049h";
    } else if (phase == 5) {
        payload = L"\x1b[?1049l";
    }
    payload += renderer_tile(8, 10, 4, {16, 220, 144});
    payload += renderer_tile(2, 2, 6, renderer_phase_colors[phase]);
    payload += renderer_tile(8, 20, 1, {16, 16, 16});
    payload += L"\x1b[2;12H" + wide_ascii(w07::calibration_marker(ascii(nonce), phase));
    // Set the measured cursor directly in the owned terminal session.
    payload += L"\x1b]12;rgb:ff/00/ff\x1b\\\x1b[8;20H\x1b[?12l\x1b[2 q";
    payload += renderer_visible[phase] ? L"\x1b[?25h" : L"\x1b[?25l";
    return payload;
}

void renderer_stamp(std::ostringstream& line, std::uint64_t timestamp) {
    line << ' ' << static_cast<std::uint32_t>(timestamp)
         << ' ' << static_cast<std::uint32_t>(timestamp >> 32);
}

std::uint64_t renderer_stamp(std::uint32_t low, std::uint32_t high) {
    return static_cast<std::uint64_t>(low) | (static_cast<std::uint64_t>(high) << 32);
}

DWORD renderer_read(HANDLE output, COORD position, std::wstring& value) {
    DWORD count{};
    require(ReadConsoleOutputCharacterW(output, value.data(), static_cast<DWORD>(value.size()),
                                        position, &count) != 0,
            "renderer active-console marker read");
    require(count == value.size(), "renderer marker read is complete");
    return count;
}

#include "renderer_archive.hpp"

struct RendererModeRestorer {
    HANDLE output;
    DWORD baseline;
    bool pending{};

    ~RendererModeRestorer() {
        if (!pending) return;
        // Failure cleanup is limited to the already authenticated owned console.
        const wchar_t reset[] = L"\x1b[?1049l\x1b[0m\x1b[?25h";
        DWORD ignored{};
        (void)WriteConsoleW(output, reset, static_cast<DWORD>(std::size(reset) - 1), &ignored, nullptr);
        (void)WriteConsoleW(output, renderer_restore_colors,
                            static_cast<DWORD>(std::size(renderer_restore_colors) - 1), &ignored, nullptr);
        (void)SetConsoleMode(output, baseline);
    }
};

SMALL_RECT renderer_console_viewport(HANDLE output) {
    CONSOLE_SCREEN_BUFFER_INFO screen{};
    require(GetConsoleScreenBufferInfo(output, &screen) != 0, "read renderer actor console viewport");
    require(screen.srWindow.Left == 0 && screen.srWindow.Top == 0
            && screen.srWindow.Right >= 55 && screen.srWindow.Bottom >= 9,
            "renderer actor viewport contains the synthetic markers");
    return screen.srWindow;
}

int renderer_actor(const std::wstring& pipe, const std::wstring& nonce, const std::wstring& profile,
                   const std::wstring& archive = {}, const std::wstring& work = {}) {
    require_renderer_nonce(nonce);
    require(pipe == renderer_pipe_name(nonce), "exact renderer pipe name");

    DWORD attached[128]{};
    const auto attached_count = GetConsoleProcessList(attached, static_cast<DWORD>(std::size(attached)));
    require(attached_count >= 1 && attached_count <= std::size(attached)
            && std::find(attached, attached + attached_count, GetCurrentProcessId()) != attached + attached_count,
            "renderer actor has positive console attachment");
    const DWORD input_access = archive.empty() ? GENERIC_READ : GENERIC_READ | GENERIC_WRITE;
    Handle input(CreateFileW(L"CONIN$", input_access, FILE_SHARE_READ | FILE_SHARE_WRITE,
                             nullptr, OPEN_EXISTING, 0, nullptr));
    Handle output(CreateFileW(L"CONOUT$", GENERIC_READ | GENERIC_WRITE, FILE_SHARE_READ | FILE_SHARE_WRITE,
                              nullptr, OPEN_EXISTING, 0, nullptr));
    require(input && output, "renderer actor owns console device handles");
    DWORD baseline_input{}, baseline_output{};
    require(GetConsoleMode(input.value, &baseline_input) && GetConsoleMode(output.value, &baseline_output),
            "renderer actor console preflight");
    const auto initial_viewport = renderer_console_viewport(output.value);
    wchar_t selected_profile[64]{};
    const auto profile_size = GetEnvironmentVariableW(L"WT_PROFILE_ID", selected_profile,
                                                      static_cast<DWORD>(std::size(selected_profile)));
    unsigned profile_state = 0;
    if (profile_size != 0) {
        profile_state = 2;
        if (profile_size < std::size(selected_profile)) {
            if (CompareStringOrdinal(selected_profile, -1, profile.c_str(), -1, TRUE) == CSTR_EQUAL) {
                profile_state = 3;
            } else if (std::wstring_view(selected_profile, profile_size)
                       == L"{00000000-0000-0000-0000-000000000000}") {
                profile_state = 1;
            }
        }
    }

    const auto nonce_text = ascii(nonce);
    std::ostringstream registered;
    registered << "R " << nonce_text << ' ' << attached_count << ' ' << baseline_input << ' ' << baseline_output
               << ' ' << static_cast<unsigned>(initial_viewport.Right + 1)
               << ' ' << static_cast<unsigned>(initial_viewport.Bottom + 1);
    renderer_stamp(registered, ticks());
    registered << ' ' << profile_state;
    // No console mutation is attempted before the outside supervisor
    // authenticates this stable client and acknowledges owned Job membership.
    send(pipe, registered.str());

    // Registration acknowledgement follows the supervisor's owned window fit.
    const auto viewport = renderer_console_viewport(output.value);
    require(archive.empty() || (viewport.Right >= 79 && viewport.Bottom >= 23),
            "archived client viewport can display the full-layout heading");
    DWORD fitted_input{}, fitted_output{};
    require(GetConsoleMode(input.value, &fitted_input) && GetConsoleMode(output.value, &fitted_output)
            && fitted_input == baseline_input && fitted_output == baseline_output,
            "renderer resize preserves the untouched console modes");
    RendererModeRestorer restore{output.value, baseline_output};
    const auto active_output = baseline_output | ENABLE_PROCESSED_OUTPUT | ENABLE_VIRTUAL_TERMINAL_PROCESSING;
    require(SetConsoleMode(output.value, active_output) != 0, "enable VT on the owned renderer console");
    restore.pending = true;
    const auto active_input = archive.empty() ? baseline_input : renderer_archive_baseline(baseline_input);
    if (!archive.empty()) {
        require(SetConsoleMode(input.value, active_input) != 0, "seed the owned archived-client cooked baseline");
    }
    const std::wstring sentinel = L"W07-" + nonce;
    const auto phase_count = archive.empty() ? renderer_stage_count : std::size_t{1};
    for (std::size_t phase = 0; phase < phase_count; ++phase) {
        auto payload = renderer_payload(nonce, phase);
        DWORD written{};
        require(WriteConsoleW(output.value, payload.data(), static_cast<DWORD>(payload.size()),
                              &written, nullptr) && written == payload.size(),
                "renderer synchronous phase write is complete");

        const auto marker = wide_ascii(w07::calibration_marker(nonce_text, phase));
        std::wstring observed_marker(marker.size(), L'\0');
        const auto marker_read = renderer_read(output.value, COORD{11, 1}, observed_marker);
        require(observed_marker == marker, "renderer active console shows the exact phase nonce marker");
        std::wstring observed_sentinel(sentinel.size(), L'\0');
        const auto sentinel_read = renderer_read(output.value, COORD{11, 3}, observed_sentinel);
        const bool saw_sentinel = observed_sentinel == sentinel;
        require(saw_sentinel == renderer_main[phase], "renderer native main/alternate sentinel transition");

        DWORD current_input{}, current_output{};
        require(GetConsoleMode(input.value, &current_input) && GetConsoleMode(output.value, &current_output)
                && current_input == active_input && current_output == active_output,
                "renderer owns the expected console mode baseline");
        const auto current_viewport = renderer_console_viewport(output.value);
        require(current_viewport.Right == viewport.Right && current_viewport.Bottom == viewport.Bottom,
                "renderer owns the stable fitted console viewport");
        std::ostringstream message;
        message << "V " << nonce_text << ' ' << phase << ' ' << written << ' ' << marker_read << ' '
                << sentinel_read << ' ' << saw_sentinel << ' ' << current_input << ' ' << current_output;
        renderer_stamp(message, ticks());
        // Stay in this frame until the actual renderer has been sampled.
        send(pipe, message.str());
    }

    if (!archive.empty()) {
        renderer_run_archive(archive, pipe, nonce, work, input.value);
        // The product's own restore was captured and authenticated above.
        // Release only the actor's initial baseline setup now.
        require(SetConsoleMode(input.value, baseline_input) != 0, "release archived-client actor baseline");
    }

    constexpr auto color_reset_length = static_cast<DWORD>(std::size(renderer_restore_colors) - 1);
    DWORD reset_written{};
    require(WriteConsoleW(output.value, renderer_restore_colors, color_reset_length, &reset_written, nullptr)
            && reset_written == color_reset_length, "restore the owned renderer colors");
    require(SetConsoleMode(output.value, baseline_output) != 0, "restore owned renderer output mode");
    DWORD final_input{}, final_output{};
    require(GetConsoleMode(input.value, &final_input) && GetConsoleMode(output.value, &final_output)
            && final_input == baseline_input && final_output == baseline_output,
            "verify owned renderer input/output restoration");
    restore.pending = false;
    std::ostringstream finished;
    finished << "F " << nonce_text << ' ' << final_input << ' ' << final_output;
    renderer_stamp(finished, ticks());
    send(pipe, finished.str());
    return 0;
}

struct RendererJobList {
    DWORD assigned{};
    DWORD count{};
    ULONG_PTR pids[32]{};
};
static_assert(offsetof(RendererJobList, pids) == offsetof(JOBOBJECT_BASIC_PROCESS_ID_LIST, ProcessIdList));

struct RendererWindow {
    HWND hwnd{};
    DWORD pid{};
    Handle process;
};

bool renderer_window_class(HWND window) {
    wchar_t name[96]{};
    const int length = GetClassNameW(window, name, static_cast<int>(std::size(name)));
    return length > 0 && std::wstring_view(name, static_cast<std::size_t>(length)) == L"CASCADIA_HOSTING_WINDOW_CLASS";
}

bool renderer_has_caption(HWND window, const std::wstring& title) {
    wchar_t caption[160]{};
    const int length = GetWindowTextW(window, caption, static_cast<int>(std::size(caption)));
    return length > 0 && std::wstring_view(caption, static_cast<std::size_t>(length)).find(title) != std::wstring_view::npos;
}

struct RendererWindowSearch {
    const RendererJobList& job_processes;
    std::array<HWND, 8> found{};
    std::size_t count{};

    static BOOL CALLBACK visit(HWND window, LPARAM raw) {
        auto& search = *reinterpret_cast<RendererWindowSearch*>(raw);
        DWORD pid{};
        if (GetWindowThreadProcessId(window, &pid) == 0 || pid == 0) return TRUE;
        const auto& pids = search.job_processes.pids;
        const auto* end = pids + search.job_processes.count;
        // Only inspect window class, visibility and pixels after an owned PID match.
        if (std::find(pids, end, static_cast<ULONG_PTR>(pid)) == end
            || !IsWindowVisible(window) || IsIconic(window) || !renderer_window_class(window)) return TRUE;
        if (search.count == search.found.size()) return FALSE;
        search.found[search.count++] = window;
        return TRUE;
    }
};

void verify_renderer_window(const RendererWindow& renderer, HANDLE job,
                            const std::wstring& terminal, const std::wstring& title) {
    exact_job(renderer.process.value, job);
    DWORD actual_pid{};
    require(WaitForSingleObject(renderer.process.value, 0) == WAIT_TIMEOUT
            && GetProcessId(renderer.process.value) == renderer.pid
            && GetWindowThreadProcessId(renderer.hwnd, &actual_pid) != 0 && actual_pid == renderer.pid
            && same_path(image(renderer.process.value), terminal) && IsWindowVisible(renderer.hwnd)
            && !IsIconic(renderer.hwnd) && renderer_window_class(renderer.hwnd)
            && renderer_has_caption(renderer.hwnd, title),
            "stable visible owned Windows Terminal window identity");
}

RendererWindow find_renderer_window(HANDLE job, const std::wstring& terminal,
                                   const std::wstring& title, Clock::time_point deadline) {
    while (Clock::now() < deadline) {
        RendererJobList processes;
        require(QueryInformationJobObject(job, JobObjectBasicProcessIdList, &processes,
                                          static_cast<DWORD>(sizeof(processes)), nullptr)
                && processes.count <= std::size(processes.pids), "bounded renderer Job PID inventory");
        RendererWindowSearch search{processes};
        require(EnumWindows(RendererWindowSearch::visit, reinterpret_cast<LPARAM>(&search)) != 0,
                "bounded owned renderer viewport enumeration");
        RendererWindow selection;
        for (std::size_t i = 0; i < search.count; ++i) {
            const auto window = search.found[i];
            DWORD pid{};
            const auto* first = processes.pids;
            const auto* end = first + processes.count;
            if (GetWindowThreadProcessId(window, &pid) == 0 || pid == 0
                || std::find(first, end, static_cast<ULONG_PTR>(pid)) == end) continue;
            Handle process(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE, FALSE, pid));
            if (!process || WaitForSingleObject(process.value, 0) != WAIT_TIMEOUT) continue;
            exact_job(process.value, job);
            if (!same_path(image(process.value), terminal) || !renderer_has_caption(window, title)) continue;
            require(!selection.hwnd, "one owned terminal window has the exact calibration title");
            selection = {window, pid, std::move(process)};
        }
        if (selection.hwnd) {
            verify_renderer_window(selection, job, terminal, title);
            return selection;
        }
        Sleep((std::min)(remaining(deadline), DWORD{20}));
    }
    SetLastError(ERROR_TIMEOUT);
    fail("owned renderer window did not become visible");
}

struct RendererFit {
    bool measured{}, reposition_requested{}, stable{};
    bool foreground_requested{}, foreground_api_succeeded{}, foreground_verified{};
    RECT work{}, before{}, requested{}, last_window{}, last_client{};
};

constexpr std::int64_t renderer_fit_width = 880;
constexpr std::int64_t renderer_fit_height = 570;
constexpr LONG renderer_fit_margin = 12;

RECT renderer_window_bounds(HWND window) {
    RECT bounds{};
    require(GetWindowRect(window, &bounds) != 0, "read owned renderer window bounds");
    return bounds;
}

RECT renderer_client_bounds(HWND window) {
    RECT bounds{};
    require(GetClientRect(window, &bounds) != 0, "read owned renderer client bounds");
    POINT first{bounds.left, bounds.top};
    POINT last{bounds.right, bounds.bottom};
    require(ClientToScreen(window, &first) && ClientToScreen(window, &last),
            "map owned renderer client bounds to physical screen coordinates");
    return {first.x, first.y, last.x, last.y};
}

bool renderer_contains(const RECT& outside, const RECT& inside) {
    return inside.right > inside.left && inside.bottom > inside.top
        && inside.left >= outside.left && inside.top >= outside.top
        && inside.right <= outside.right && inside.bottom <= outside.bottom;
}

RECT renderer_fit_target(const RECT& window, const RECT& work) {
    const auto available_width = static_cast<std::int64_t>(work.right) - work.left - 2 * renderer_fit_margin;
    const auto available_height = static_cast<std::int64_t>(work.bottom) - work.top - 2 * renderer_fit_margin;
    const auto width = std::min({static_cast<std::int64_t>(window.right) - window.left,
                                 available_width, renderer_fit_width});
    const auto height = std::min({static_cast<std::int64_t>(window.bottom) - window.top,
                                  available_height, renderer_fit_height});
    require(width > 0 && height > 0, "renderer monitor work area has room for an owned window fit");
    const auto left = std::clamp(static_cast<std::int64_t>(window.left),
        static_cast<std::int64_t>(work.left) + renderer_fit_margin,
        static_cast<std::int64_t>(work.right) - renderer_fit_margin - width);
    const auto top = std::clamp(static_cast<std::int64_t>(window.top),
        static_cast<std::int64_t>(work.top) + renderer_fit_margin,
        static_cast<std::int64_t>(work.bottom) - renderer_fit_margin - height);
    return {static_cast<LONG>(left), static_cast<LONG>(top),
            static_cast<LONG>(left + width), static_cast<LONG>(top + height)};
}

void fit_renderer_window(const RendererWindow& renderer, HANDLE job, const std::wstring& terminal,
                         const std::wstring& title, Clock::time_point work_deadline, RendererFit& fit,
                         bool foreground_required) {
    using namespace renderer_pixels_detail;
    PixelCoordinates coordinates;
    const auto deadline = (std::min)(work_deadline, Clock::now() + 5s);
    verify_renderer_window(renderer, job, terminal, title);
    if (foreground_required) {
        if (GetForegroundWindow() != renderer.hwnd) {
            // The hosted desktop may show an owned window without activating it.
            // Request activation once and retain the real foreground observation.
            fit.foreground_requested = true;
            fit.foreground_api_succeeded = SetForegroundWindow(renderer.hwnd) != FALSE;
        }
        while (GetForegroundWindow() != renderer.hwnd && Clock::now() < deadline)
            Sleep(remaining(deadline, 20));
        verify_renderer_window(renderer, job, terminal, title);
        require_foreground(renderer.hwnd);
        fit.foreground_verified = true;
    }
    fit.before = fit.last_window = renderer_window_bounds(renderer.hwnd);
    fit.last_client = renderer_client_bounds(renderer.hwnd);
    fit.measured = true;
    const auto monitor = MonitorFromWindow(renderer.hwnd, MONITOR_DEFAULTTONULL);
    MONITORINFO info{};
    info.cbSize = static_cast<DWORD>(sizeof(info));
    require(monitor && GetMonitorInfoW(monitor, &info), "find owned renderer's current monitor work area");
    fit.work = info.rcWork;
    fit.requested = renderer_fit_target(fit.before, fit.work);

    if (!same_rectangle(fit.before, fit.requested)) {
        require(Clock::now() < deadline, "renderer window fit deadline before reposition");
        verify_renderer_window(renderer, job, terminal, title);
        if (foreground_required) require_foreground(renderer.hwnd);
        fit.reposition_requested = true;
        require(SetWindowPos(renderer.hwnd, nullptr, fit.requested.left, fit.requested.top,
                             fit.requested.right - fit.requested.left, fit.requested.bottom - fit.requested.top,
                             SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOOWNERZORDER | SWP_ASYNCWINDOWPOS) != 0,
                "fit only the verified owned renderer window");
        verify_renderer_window(renderer, job, terminal, title);
    }

    auto stable_since = Clock::now();
    while (Clock::now() < deadline) {
        if (foreground_required) require_foreground(renderer.hwnd);
        else verify_renderer_window(renderer, job, terminal, title);
        const auto window = renderer_window_bounds(renderer.hwnd);
        const auto client = renderer_client_bounds(renderer.hwnd);
        const bool unchanged = same_rectangle(window, fit.last_window) && same_rectangle(client, fit.last_client);
        fit.last_window = window;
        fit.last_client = client;
        const bool fitted = renderer_contains(fit.work, window) && renderer_contains(window, client)
            && static_cast<std::int64_t>(window.right) - window.left <= renderer_fit_width
            && static_cast<std::int64_t>(window.bottom) - window.top <= renderer_fit_height
            && MonitorFromWindow(renderer.hwnd, MONITOR_DEFAULTTONULL) == monitor
            && (!fit.reposition_requested || !same_rectangle(window, fit.before));
        if (!fitted || !unchanged) {
            stable_since = Clock::now();
        } else if (Clock::now() - stable_since >= 100ms
                   && same_rectangle(foreground_required ? visible_client(renderer.hwnd)
                                                         : renderer_client_bounds(renderer.hwnd), client)
                   && same_rectangle(renderer_window_bounds(renderer.hwnd), window)) {
            verify_renderer_window(renderer, job, terminal, title);
            coordinates.restore();
            fit.stable = true;
            return;
        }
        Sleep(remaining(deadline, 20));
    }
    SetLastError(ERROR_TIMEOUT);
    fail("owned renderer did not settle within its monitor work area");
}

struct RendererPhase {
    std::size_t index{};
    std::uint32_t written{}, marker_read{}, sentinel_read{}, input{}, output{};
    bool native_main{};
    std::uint64_t actor_qpc{}, capture_qpc{}, ack_qpc{};
    RendererPixelObservation pixels{};
};

struct RendererReceipt {
    bool passed{}, launcher_owned{}, actor_waited{}, launcher_waited{}, renderer_waited{};
    bool job_empty{}, pipe_quiescent{}, cleanup_forced{}, modes_restored{}, window_capture{};
    DWORD launcher_pid{}, actor_pid{}, renderer_pid{}, launcher_exit{STILL_ACTIVE}, actor_exit{STILL_ACTIVE};
    DWORD initial_input{}, initial_output{}, restored_input{}, restored_output{}, associated_consoles{};
    DWORD columns{}, rows{}, job_remaining{}, profile_state{};
    std::uint64_t start{}, registered{}, restore_qpc{}, end{}, control_bytes{};
    std::string failure;
    RendererFit fit;
    ignatius_renderer_modal::ModalReceipt modal;
    std::vector<RendererPhase> phases;
    std::optional<RendererArchiveReceipt> archive;
};

template<std::size_t count>
std::array<std::uint32_t, count> renderer_numbers(const std::vector<std::string_view>& fields) {
    require(fields.size() == count + 2, "structured renderer message width");
    std::array<std::uint32_t, count> values{};
    for (std::size_t i = 0; i < count; ++i) {
        require(w07::number(fields[i + 2], values[i]), "structured renderer message integer");
    }
    return values;
}

DWORD renderer_active_processes(HANDLE job) {
    JOBOBJECT_BASIC_ACCOUNTING_INFORMATION accounting{};
    require(QueryInformationJobObject(job, JobObjectBasicAccountingInformation, &accounting,
                                      static_cast<DWORD>(sizeof(accounting)), nullptr),
            "query exact renderer Job account");
    return accounting.ActiveProcesses;
}

void renderer_wait_job(HANDLE job, bool failed, Clock::time_point deadline, RendererReceipt& receipt) {
    auto drain = (std::min)(deadline, Clock::now() + 500ms);
    receipt.job_remaining = renderer_active_processes(job);
    while (!failed && receipt.job_remaining != 0 && Clock::now() < drain) {
        Sleep(5);
        receipt.job_remaining = renderer_active_processes(job);
    }
    if (receipt.job_remaining != 0) {
        require(TerminateJobObject(job, 125) != 0, "terminate only the exact renderer Job");
        receipt.cleanup_forced = true;
        while (Clock::now() < deadline) {
            receipt.job_remaining = renderer_active_processes(job);
            if (receipt.job_remaining == 0) break;
            Sleep(5);
        }
    }
    receipt.job_empty = receipt.job_remaining == 0;
}

std::string renderer_json_message(std::string_view message) {
    std::string escaped;
    for (char c : message.substr(0, 512)) {
        if (c == '\\' || c == '"') escaped += '\\';
        escaped += c >= ' ' && c <= '~' ? c : '?';
    }
    return escaped;
}

std::string renderer_json_rectangle(const RECT& rect) {
    return "[" + std::to_string(rect.left) + "," + std::to_string(rect.top)
        + "," + std::to_string(rect.right) + "," + std::to_string(rect.bottom) + "]";
}

void emit_renderer_receipt(const RendererReceipt& receipt) {
    const auto truth = [](bool value) { return value ? "true" : "false"; };
    std::cout << "{\"schema\":1,\"case\":\""
              << (receipt.archive ? "renderer-archive" : "renderer-calibration") << "\",\"native_windows\":true"
              << ",\"calibration_complete\":" << truth(receipt.passed && !receipt.archive)
              << ",\"product_acceptance\":false,\"cursor_capability_accepted\":false"
              << ",\"capture_backend\":\"" << (receipt.window_capture ? "wgc-owned-window" : "gdi-owned-client") << '"'
              << ",\"cursor_scope\":\""
              << (receipt.window_capture ? "unmeasured" : receipt.archive ? "focused-main" : "focused-controls") << '"'
              << ",\"qpc_frequency\":" << frequency()
              << ",\"start_qpc\":" << receipt.start << ",\"end_qpc\":" << receipt.end
              << ",\"failure\":\"" << renderer_json_message(receipt.failure) << '"'
              << ",\"launcher_pid\":" << receipt.launcher_pid << ",\"renderer_pid\":" << receipt.renderer_pid
              << ",\"actor_pid\":" << receipt.actor_pid << ",\"console_processes\":" << receipt.associated_consoles
              << ",\"actor_registered_qpc\":" << receipt.registered
              << ",\"observed_profile\":\"" << renderer_profile_states[receipt.profile_state] << '"'
              << ",\"settings_dialog\":{\"category\":\"" << receipt.modal.category << '"'
              << ",\"automation_hresult\":" << receipt.modal.automation_hresult
              << ",\"settings_hresult\":" << receipt.modal.settings_hresult
              << ",\"json_line\":" << receipt.modal.json.line
              << ",\"json_column\":" << receipt.modal.json.column << '}'
              << ",\"columns\":" << receipt.columns << ",\"rows\":" << receipt.rows
              << ",\"control_bytes\":" << receipt.control_bytes
              << ",\"fit\":{\"measured\":" << truth(receipt.fit.measured)
              << ",\"foreground_requested\":" << truth(receipt.fit.foreground_requested)
              << ",\"foreground_api_succeeded\":" << truth(receipt.fit.foreground_api_succeeded)
              << ",\"foreground_verified\":" << truth(receipt.fit.foreground_verified)
              << ",\"reposition_requested\":" << truth(receipt.fit.reposition_requested)
              << ",\"stable\":" << truth(receipt.fit.stable)
              << ",\"work_area\":" << renderer_json_rectangle(receipt.fit.work)
              << ",\"before_window\":" << renderer_json_rectangle(receipt.fit.before)
              << ",\"requested_window\":" << renderer_json_rectangle(receipt.fit.requested)
              << ",\"last_window\":" << renderer_json_rectangle(receipt.fit.last_window)
              << ",\"last_client\":" << renderer_json_rectangle(receipt.fit.last_client) << '}'
              << ",\"modes\":{\"before_input\":" << receipt.initial_input
              << ",\"before_output\":" << receipt.initial_output
              << ",\"restored_input\":" << receipt.restored_input
              << ",\"restored_output\":" << receipt.restored_output
              << ",\"restored\":" << truth(receipt.modes_restored)
              << ",\"restored_qpc\":" << receipt.restore_qpc << '}'
              << ",\"cleanup\":{\"launcher_owned\":" << truth(receipt.launcher_owned)
              << ",\"actor_waited\":" << truth(receipt.actor_waited)
              << ",\"actor_exit\":" << receipt.actor_exit
              << ",\"launcher_waited\":" << truth(receipt.launcher_waited)
              << ",\"launcher_exit\":" << receipt.launcher_exit
              << ",\"renderer_waited\":" << truth(receipt.renderer_waited)
              << ",\"job_remaining\":" << receipt.job_remaining
              << ",\"job_empty\":" << truth(receipt.job_empty)
              << ",\"forced_owned_job_cleanup\":" << truth(receipt.cleanup_forced)
              << ",\"pipe_quiescent\":" << truth(receipt.pipe_quiescent) << '}';
    if (receipt.archive) {
        std::cout << ",\"archive_diagnostic_passed\":" << truth(receipt.passed);
        emit_renderer_archive(*receipt.archive, receipt.window_capture);
    }
    std::cout << ",\"phases\":[";
    bool first = true;
    for (const auto& phase : receipt.phases) {
        if (!first) std::cout << ',';
        first = false;
        const auto& pixel = phase.pixels;
        std::cout << "{\"index\":" << phase.index
                  << ",\"name\":\"" << renderer_stage_names[phase.index] << '"'
                  << ",\"expected_visible\":" << (receipt.window_capture ? "null" : truth(renderer_visible[phase.index]))
                  << ",\"expected_main\":" << truth(renderer_main[phase.index])
                  << ",\"native_main\":" << truth(phase.native_main)
                  << ",\"native_written\":" << phase.written
                  << ",\"native_marker_read\":" << phase.marker_read
                  << ",\"native_sentinel_read\":" << phase.sentinel_read
                  << ",\"native_input\":" << phase.input << ",\"native_output\":" << phase.output
                  << ",\"actor_qpc\":" << phase.actor_qpc
                  << ",\"capture_qpc\":" << phase.capture_qpc << ",\"ack_qpc\":" << phase.ack_qpc
                  << ",\"crop\":\"" << (receipt.archive ? "baseline.bmp" : "phase-" + std::to_string(phase.index) + ".bmp") << '"'
                  << ",\"pixels\":{\"client_width\":" << pixel.client_width
                  << ",\"client_height\":" << pixel.client_height
                  << ",\"cell_width\":" << pixel.cell_width
                  << ",\"cell_height\":" << pixel.cell_height
                  << ",\"crop_width\":" << pixel.crop_width
                  << ",\"crop_height\":" << pixel.crop_height
                  << ",\"anchor\":" << pixel.anchor_pixels
                  << ",\"phase\":" << pixel.phase_pixels
                  << ",\"sentinel\":" << pixel.sentinel_pixels
                  << ",\"cursor\":" << (receipt.window_capture ? "null" : std::to_string(pixel.cursor_pixels))
                  << ",\"cursor_area\":" << (receipt.window_capture ? "null" : std::to_string(pixel.cursor_area))
                  << ",\"stable_frames\":" << pixel.stable_frames << "}}";
    }
    std::cout << "]}\n";
}

int renderer_supervise(const std::wstring& terminal, const std::wstring& work, const std::wstring& nonce,
                       const std::wstring& profile, const std::wstring& archive = {}, bool window_capture = false) {
    RendererReceipt receipt;
    if (!archive.empty()) receipt.archive.emplace();
    receipt.window_capture = window_capture;
    const auto phase_count = receipt.archive ? std::size_t{1} : renderer_stage_count;
    receipt.start = ticks();
    const auto start = Clock::now();
    const auto deadline = start + 30s;
    const auto work_deadline = start + 25s;
    std::unique_ptr<Server> server;
    Handle job, actor, archived_client;
    Child launcher;
    RendererWindow renderer;
#ifdef IGNATIUS_RENDERER_WINDOW_CAPTURE
    std::optional<ignatius_wgc::WindowCapture> owned_capture;
#endif
    bool observed_all = false;
    try {
        require(!window_capture || receipt.archive.has_value(), "WGC requires the owned archive route");
#ifndef IGNATIUS_RENDERER_WINDOW_CAPTURE
        require(!window_capture, "WGC is not compiled in this renderer helper");
#endif
        require_renderer_nonce(nonce);
        const auto folder = std::filesystem::path(work);
        const auto executable = std::filesystem::path(terminal);
        require(folder.is_absolute() && std::filesystem::is_directory(folder),
                "renderer probe requires a fresh absolute owned work directory");
        require(executable.is_absolute() && same_path(executable.filename().wstring(), L"WindowsTerminal.exe")
                && std::filesystem::is_regular_file(executable.parent_path() / L".portable")
                && std::filesystem::is_regular_file(executable.parent_path() / L"settings" / L"settings.json"),
                "renderer probe requires the isolated portable distribution");
        if (receipt.archive) {
            const auto client = std::filesystem::path(archive);
            require(client.is_absolute() && same_path(client.filename().wstring(), L"ignatius.exe")
                    && std::filesystem::is_regular_file(client),
                    "archived client is an absolute verified executable");
        }
        const auto title = L"W07Renderer-" + nonce;
        const auto pipe = renderer_pipe_name(nonce);
        const auto nonce_text = ascii(nonce);
        const auto executable_self = self();
        job = Handle(CreateJobObjectW(nullptr, nullptr));
        require(static_cast<bool>(job), "create renderer Job");
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION limits{};
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        require(SetInformationJobObject(job.value, JobObjectExtendedLimitInformation,
                                        &limits, static_cast<DWORD>(sizeof(limits))),
                "set sole-owner renderer Job kill-on-close");
        Security security;
        server = std::make_unique<Server>(pipe, security, deadline);

        STARTUPINFOW startup{};
        startup.cb = sizeof(startup);
        require(profile.size() == 38 && profile.front() == L'{' && profile.back() == L'}',
                "owned renderer profile identity");
        std::vector<std::wstring> arguments{L"-w", L"new", L"new-tab", L"--profile", profile, L"--title", title,
            L"--suppressApplicationTitle", L"--inheritEnvironment", L"--", executable_self,
            receipt.archive ? L"--renderer-archive-actor" : L"--renderer-actor", pipe, nonce, profile};
        if (receipt.archive) {
            arguments.push_back(archive);
            arguments.push_back(work);
        }
        launcher = create(terminal, std::move(arguments), work, &startup, 0, renderer_environment(folder));
        receipt.launcher_pid = launcher.pid;
        require(AssignProcessToJobObject(job.value, launcher.process.value),
                "assign suspended Terminal to the exact owned Job");
        receipt.launcher_owned = true;
        exact_job(launcher.process.value, job.value);
        const auto launched_birth = birth(launcher.process.value);
        require(same_path(image(launcher.process.value), terminal), "exact suspended portable launcher image");
        const auto resumed = ticks();
        resume(launcher);

        for (;;) {
            require(Clock::now() < work_deadline, "renderer overall work deadline");
            server->connect(work_deadline);
            const auto pid = server->client();
            const DWORD access = PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE
                | (receipt.archive ? PROCESS_DUP_HANDLE : 0);
            Handle sender(OpenProcess(access, FALSE, pid));
            require(static_cast<bool>(sender), "open stable renderer actor process");
            exact_job(sender.value, job.value);
            const auto sender_birth = birth(sender.value);
            require(pid != launcher.pid && GetProcessId(sender.value) == pid
                    && WaitForSingleObject(sender.value, 0) == WAIT_TIMEOUT
                    && same_path(image(sender.value), executable_self)
                    && CompareFileTime(&sender_birth, &launched_birth) >= 0,
                    "exact owned console actor identity");
            const auto line = server->read(work_deadline);
            const auto parts = w07::words(line);
            require(parts.size() >= 2 && parts[1] == nonce_text, "renderer control nonce and message shape");

            if (parts[0] == "R") {
                require(!actor && receipt.phases.empty(), "single persistent renderer actor");
                const auto data = renderer_numbers<8>(parts);
                receipt.associated_consoles = data[0];
                receipt.initial_input = data[1];
                receipt.initial_output = data[2];
                receipt.columns = data[3];
                receipt.rows = data[4];
                receipt.registered = renderer_stamp(data[5], data[6]);
                require(receipt.associated_consoles >= 1 && receipt.associated_consoles <= 128
                        && receipt.columns >= 56 && receipt.columns <= 200
                        && receipt.rows >= 10 && receipt.rows <= 80
                        && data[7] < renderer_profile_states.size()
                        && receipt.registered >= resumed && receipt.registered <= ticks(),
                        "renderer actor registration and viewport bounds");
                require(!receipt.archive || (receipt.columns >= 80 && receipt.rows >= 24),
                        "archived client starts with a full-size console");
                receipt.profile_state = data[7];
                receipt.actor_pid = pid;
                actor = std::move(sender);
                renderer = find_renderer_window(job.value, terminal, title, work_deadline);
                receipt.renderer_pid = renderer.pid;
                fit_renderer_window(renderer, job.value, terminal, title, work_deadline, receipt.fit, !window_capture);
                if (receipt.profile_state != 3) {
                    receipt.modal = ignatius_renderer_modal::inspect(renderer.hwnd, renderer.pid, work_deadline);
                    verify_renderer_window(renderer, job.value, terminal, title);
                    if (std::string_view(receipt.modal.category) == "settings_load_error")
                        throw std::runtime_error("owned portable Terminal fell back after a settings load error");
                }
                server->ack(work_deadline);
                continue;
            }
            require(static_cast<bool>(actor) && pid == receipt.actor_pid
                    && GetProcessId(actor.value) == pid && WaitForSingleObject(actor.value, 0) == WAIT_TIMEOUT,
                    "one stable renderer actor for all phases");
            exact_job(actor.value, job.value);
            const auto actor_birth = birth(actor.value);
            require(CompareFileTime(&sender_birth, &actor_birth) == 0, "renderer actor creation identity");
            const auto prior_qpc = receipt.phases.empty() ? receipt.registered : receipt.phases.back().ack_qpc;

            if (receipt.archive && parts[0] == "P") {
                require(!archived_client && receipt.phases.size() == phase_count,
                        "archived client starts after the measured baseline");
                archived_client = renderer_register_archive(sender.value, job.value, archive,
                    renderer_numbers<5>(parts), *receipt.archive);
                server->ack(work_deadline);
                continue;
            }
            if (receipt.archive && parts[0] == "E") {
                auto& product = *receipt.archive;
                const auto data = renderer_numbers<4>(parts);
                require(archived_client && !product.raw_alternate && data[0] <= 15,
                        "bounded archive startup failure observation");
                product.startup_facts = data[0];
                product.startup_last_input = data[1];
                product.startup_last_output = data[2];
                product.startup_early_exit = data[3];
                server->ack(work_deadline);
                throw std::runtime_error("archived client did not reach the native startup checkpoint");
            }
            if (receipt.archive && parts[0] == "N") {
                auto& product = *receipt.archive;
                const auto data = renderer_numbers<4>(parts);
                require(archived_client && !product.raw_alternate
                        && WaitForSingleObject(archived_client.value, 0) == WAIT_TIMEOUT,
                        "archived product is still executing in the owned console");
                product.acquired_input = data[0];
                product.acquired_output = data[1];
                product.acquired_qpc = renderer_stamp(data[2], data[3]);
                require((product.acquired_input & renderer_cooked_bits) == 0
                        && (product.acquired_output & ENABLE_VIRTUAL_TERMINAL_PROCESSING) != 0
                        && product.acquired_qpc >= prior_qpc && product.acquired_qpc <= ticks(),
                        "authenticated native raw alternate-screen checkpoint");
                product.raw_alternate = true;
                verify_renderer_window(renderer, job.value, terminal, title);
#ifdef IGNATIUS_RENDERER_WINDOW_CAPTURE
                if (window_capture) {
                    require(owned_capture.has_value(), "WGC archive activation follows its baseline capture");
                    owned_capture->restart(work_deadline);
                    renderer_capture_archive_window(*owned_capture, folder / L"ignatius-active.bmp",
                                                    prior_qpc, work_deadline, product);
                } else
#endif
                {
                    renderer_capture_archive(renderer.hwnd, folder / L"ignatius-active.bmp", work_deadline, product);
                }
                verify_renderer_window(renderer, job.value, terminal, title);
                product.acquired_capture_qpc = ticks();
                server->ack(work_deadline);
                continue;
            }
            if (receipt.archive && parts[0] == "T") {
                auto& product = *receipt.archive;
                const auto data = renderer_numbers<6>(parts);
                require(archived_client && product.acquired_capture_qpc != 0 && !product.waited,
                        "archived product restoration follows the measured acquisition");
                product.waited = WaitForSingleObject(archived_client.value, 0) == WAIT_OBJECT_0;
                require(product.waited && GetExitCodeProcess(archived_client.value, &product.exit_code)
                        && product.exit_code == data[2] && product.exit_code == 0,
                        "archived product actually exited zero after Ctrl+Q");
                product.restored_input = data[0];
                product.restored_output = data[1];
                product.restored_sentinel = data[3] == 1;
                product.restored_qpc = renderer_stamp(data[4], data[5]);
                const auto& baseline = receipt.phases.front();
                product.restored_modes = product.restored_input == baseline.input
                    && product.restored_output == baseline.output;
                require(product.restored_sentinel && product.restored_modes
                        && product.restored_qpc >= product.acquired_capture_qpc && product.restored_qpc <= ticks(),
                        "archived product restored the native main sentinel and seeded mode baseline");
                verify_renderer_window(renderer, job.value, terminal, title);
#ifdef IGNATIUS_RENDERER_WINDOW_CAPTURE
                if (window_capture) {
                    require(owned_capture.has_value(), "WGC archive restoration follows its baseline capture");
                    owned_capture->restart(work_deadline);
                    product.restored_pixels = observe_archive_main_window(*owned_capture, folder / L"ignatius-restored.bmp",
                        product.acquired_capture_qpc, work_deadline, product.wgc_restored);
                    owned_capture->close();
                } else
#endif
                {
                    product.restored_pixels = observe_renderer_frame(renderer.hwnd, 0, true, true, work_deadline,
                        folder / L"ignatius-restored.bmp");
                }
                verify_renderer_window(renderer, job.value, terminal, title);
                product.restored_capture_qpc = ticks();
                server->ack(work_deadline);
                continue;
            }

            if (parts[0] == "V") {
                const auto data = renderer_numbers<9>(parts);
                const auto index = receipt.phases.size();
                require(index < phase_count && data[0] == index, "strict renderer phase ordering");
                RendererPhase phase;
                phase.index = index;
                phase.written = data[1];
                phase.marker_read = data[2];
                phase.sentinel_read = data[3];
                phase.native_main = data[4] == 1;
                phase.input = data[5];
                phase.output = data[6];
                phase.actor_qpc = renderer_stamp(data[7], data[8]);
                require(phase.written == renderer_payload(nonce, index).size()
                        && phase.marker_read == w07::calibration_marker(nonce_text, index).size()
                        && phase.sentinel_read == 4 + nonce.size() && data[4] <= 1
                        && phase.native_main == renderer_main[index]
                        && phase.input == (receipt.archive ? renderer_archive_baseline(receipt.initial_input) : receipt.initial_input)
                        && phase.output == (receipt.initial_output | ENABLE_PROCESSED_OUTPUT | ENABLE_VIRTUAL_TERMINAL_PROCESSING)
                        && phase.actor_qpc >= prior_qpc && phase.actor_qpc <= ticks(),
                        "renderer native write, marker, buffer and mode evidence");

                verify_renderer_window(renderer, job.value, terminal, title);
#ifdef IGNATIUS_RENDERER_WINDOW_CAPTURE
                if (window_capture) {
                    require(index == 0 && receipt.archive && !owned_capture, "WGC captures exactly one archive baseline");
                    owned_capture.emplace(renderer.hwnd, renderer.process.value, [&] {
                        verify_renderer_window(renderer, job.value, terminal, title);
                    }, work_deadline);
                    receipt.archive->wgc_software_device = owned_capture->uses_software_device();
                    phase.pixels = observe_archive_main_window(*owned_capture, folder / L"baseline.bmp",
                        prior_qpc, work_deadline, receipt.archive->wgc_baseline);
                } else
#endif
                {
                    phase.pixels = observe_renderer_frame(renderer.hwnd, index, renderer_visible[index],
                        renderer_main[index], work_deadline,
                        folder / (receipt.archive ? L"baseline.bmp" : L"phase-" + std::to_wstring(index) + L".bmp"));
                }
                verify_renderer_window(renderer, job.value, terminal, title);
                phase.capture_qpc = ticks();
                require(phase.capture_qpc >= phase.actor_qpc && Clock::now() < work_deadline,
                        "verified renderer frame precedes the actor acknowledgement");
                // The next phase can start as soon as ACK arrives. Stamp
                // before sending it, then require the bounded client receipt.
                phase.ack_qpc = ticks();
                server->ack(work_deadline);
                receipt.phases.push_back(phase);
                continue;
            }
            require(parts[0] == "F" && receipt.phases.size() == phase_count,
                    "renderer final restoration follows all measured phases");
            require(!receipt.archive || receipt.archive->restored_capture_qpc != 0,
                    "archive final actor cleanup follows product restoration");
            const auto data = renderer_numbers<4>(parts);
            receipt.restored_input = data[0];
            receipt.restored_output = data[1];
            receipt.restore_qpc = renderer_stamp(data[2], data[3]);
            receipt.modes_restored = receipt.restored_input == receipt.initial_input
                && receipt.restored_output == receipt.initial_output;
            const auto final_prior = receipt.archive ? receipt.archive->restored_capture_qpc : prior_qpc;
            require(receipt.modes_restored && receipt.restore_qpc >= final_prior && receipt.restore_qpc <= ticks(),
                    "renderer restoration receipt proves the original console masks");
            server->ack(work_deadline);
            break;
        }
        receipt.actor_waited = WaitForSingleObject(actor.value, remaining(work_deadline, 25000)) == WAIT_OBJECT_0;
        require(receipt.actor_waited && GetExitCodeProcess(actor.value, &receipt.actor_exit)
                && receipt.actor_exit == 0, "renderer actor actually completed and exited zero");
        observed_all = true;
    } catch (const std::exception& error) {
        receipt.failure = error.what();
    }

    try {
        if (launcher.process && !receipt.launcher_owned) {
            // The unassigned launcher was never resumed. Its specific process
            // handle belongs to this invocation, and no by-PID kill is used.
            require(TerminateProcess(launcher.process.value, 125), "terminate owned suspended unassigned launcher");
        }
        if (job) renderer_wait_job(job.value, !observed_all, deadline, receipt);

        if (launcher.process) {
            receipt.launcher_waited = WaitForSingleObject(launcher.process.value, remaining(deadline, 500)) == WAIT_OBJECT_0;
            require(receipt.launcher_waited && GetExitCodeProcess(launcher.process.value, &receipt.launcher_exit),
                    "wait for exact released renderer launcher handle");
        }
        if (renderer.process) {
            receipt.renderer_waited = WaitForSingleObject(renderer.process.value, remaining(deadline, 500)) == WAIT_OBJECT_0;
            require(receipt.renderer_waited, "wait for exact renderer process handle");
        }
        if (actor && !receipt.actor_waited) {
            receipt.actor_waited = WaitForSingleObject(actor.value, remaining(deadline, 500)) == WAIT_OBJECT_0;
            require(receipt.actor_waited && GetExitCodeProcess(actor.value, &receipt.actor_exit),
                    "wait for exact renderer actor after failure");
        }
        if (archived_client && !receipt.archive->waited) {
            receipt.archive->waited = WaitForSingleObject(archived_client.value, remaining(deadline, 500)) == WAIT_OBJECT_0;
            require(receipt.archive->waited && GetExitCodeProcess(archived_client.value, &receipt.archive->exit_code),
                    "wait for the exact archived product after failure");
        }
        receipt.pipe_quiescent = server && server->quiescent();
        require(receipt.job_empty && receipt.pipe_quiescent, "renderer cleanup completed for Job and control pipe");
    } catch (const std::exception& error) {
        if (receipt.failure.empty()) receipt.failure = error.what();
        else receipt.failure += " | cleanup incomplete";
    }
    receipt.control_bytes = server ? server->bytes() : 0;
    receipt.end = ticks();
    receipt.passed = observed_all && receipt.failure.empty() && receipt.launcher_owned
        && receipt.launcher_waited && receipt.renderer_waited && receipt.actor_waited
        && receipt.job_empty && receipt.pipe_quiescent && receipt.fit.stable;
    emit_renderer_receipt(receipt);
    return receipt.passed ? 0 : 1;
}
