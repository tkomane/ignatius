// Test-only synthetic Windows Terminal calibration. Included in the
// Windows host's anonymous namespace, after renderer_pixels.hpp.
constexpr std::size_t renderer_stage_count = 6;
constexpr std::array<const char*, renderer_stage_count> renderer_stage_names{
    "main-show", "main-hide", "alt-enter-hidden", "alt-show", "alt-hide", "restored-main"
};
constexpr std::array<bool, renderer_stage_count> renderer_visible{true, false, false, true, false, true};
constexpr std::array<bool, renderer_stage_count> renderer_main{true, true, false, false, false, true};

std::wstring renderer_pipe_name(const std::wstring& nonce) {
    return L"\\\\.\\pipe\\ignatius-w07-renderer-" + nonce;
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
        payload = L"\x1b[0m\x1b[2J";
        payload += renderer_tile(4, 2, 6, {125, 32, 220});
        payload += L"\x1b[4;12HW07-" + nonce;
    } else if (phase == 2) {
        payload = L"\x1b[?1049h";
    } else if (phase == 5) {
        payload = L"\x1b[?1049l";
    }
    payload += renderer_tile(8, 10, 4, {16, 220, 144});
    payload += renderer_tile(2, 2, 6, renderer_phase_colors[phase]);
    payload += L"\x1b[2;12H" + wide_ascii(w07::calibration_marker(ascii(nonce), phase));
    // The isolated profile also supplies a filled-box magenta cursor.
    payload += L"\x1b[8;20H\x1b[?12l\x1b[2 q";
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
        (void)SetConsoleMode(output, baseline);
    }
};

int renderer_actor(const std::wstring& pipe, const std::wstring& nonce) {
    require_renderer_nonce(nonce);
    require(pipe == renderer_pipe_name(nonce), "exact renderer pipe name");

    DWORD attached[128]{};
    const auto attached_count = GetConsoleProcessList(attached, static_cast<DWORD>(std::size(attached)));
    require(attached_count >= 1 && attached_count <= std::size(attached)
            && std::find(attached, attached + attached_count, GetCurrentProcessId()) != attached + attached_count,
            "renderer actor has positive console attachment");
    Handle input(CreateFileW(L"CONIN$", GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE,
                             nullptr, OPEN_EXISTING, 0, nullptr));
    Handle output(CreateFileW(L"CONOUT$", GENERIC_READ | GENERIC_WRITE, FILE_SHARE_READ | FILE_SHARE_WRITE,
                              nullptr, OPEN_EXISTING, 0, nullptr));
    require(input && output, "renderer actor owns console device handles");
    DWORD baseline_input{}, baseline_output{};
    CONSOLE_SCREEN_BUFFER_INFO screen{};
    require(GetConsoleMode(input.value, &baseline_input) && GetConsoleMode(output.value, &baseline_output)
            && GetConsoleScreenBufferInfo(output.value, &screen), "renderer actor console preflight");
    require(screen.srWindow.Left == 0 && screen.srWindow.Top == 0
            && screen.srWindow.Right >= 55 && screen.srWindow.Bottom >= 9,
            "renderer actor viewport contains the synthetic markers");

    const auto nonce_text = ascii(nonce);
    std::ostringstream registered;
    registered << "R " << nonce_text << ' ' << attached_count << ' ' << baseline_input << ' ' << baseline_output
               << ' ' << static_cast<unsigned>(screen.srWindow.Right + 1)
               << ' ' << static_cast<unsigned>(screen.srWindow.Bottom + 1);
    renderer_stamp(registered, ticks());
    // No console mutation is attempted before the outside supervisor
    // authenticates this stable client and acknowledges owned Job membership.
    send(pipe, registered.str());

    RendererModeRestorer restore{output.value, baseline_output};
    const auto active_output = baseline_output | ENABLE_PROCESSED_OUTPUT | ENABLE_VIRTUAL_TERMINAL_PROCESSING;
    require(SetConsoleMode(output.value, active_output) != 0, "enable VT on the owned renderer console");
    restore.pending = true;
    const std::wstring sentinel = L"W07-" + nonce;
    for (std::size_t phase = 0; phase < renderer_stage_count; ++phase) {
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
                && current_input == baseline_input && current_output == active_output,
                "renderer owns the expected console mode baseline");
        std::ostringstream message;
        message << "V " << nonce_text << ' ' << phase << ' ' << written << ' ' << marker_read << ' '
                << sentinel_read << ' ' << saw_sentinel << ' ' << current_input << ' ' << current_output;
        renderer_stamp(message, ticks());
        // Stay in this frame until the actual renderer has been sampled.
        send(pipe, message.str());
    }

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

struct RendererPhase {
    std::size_t index{};
    std::uint32_t written{}, marker_read{}, sentinel_read{}, input{}, output{};
    bool native_main{};
    std::uint64_t actor_qpc{}, capture_qpc{}, ack_qpc{};
    RendererPixelObservation pixels{};
};

struct RendererReceipt {
    bool passed{}, launcher_owned{}, actor_waited{}, launcher_waited{}, renderer_waited{};
    bool job_empty{}, pipe_quiescent{}, cleanup_forced{}, modes_restored{};
    DWORD launcher_pid{}, actor_pid{}, renderer_pid{}, launcher_exit{STILL_ACTIVE}, actor_exit{STILL_ACTIVE};
    DWORD initial_input{}, initial_output{}, restored_input{}, restored_output{}, associated_consoles{};
    DWORD columns{}, rows{}, job_remaining{};
    std::uint64_t start{}, registered{}, restore_qpc{}, end{}, control_bytes{};
    std::string failure;
    std::vector<RendererPhase> phases;
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

void emit_renderer_receipt(const RendererReceipt& receipt) {
    std::cout << "{\"schema\":1,\"case\":\"renderer-calibration\",\"native_windows\":true"
              << ",\"calibration_complete\":" << truth(receipt.passed)
              << ",\"product_acceptance\":false,\"cursor_capability_accepted\":false"
              << ",\"qpc_frequency\":" << frequency()
              << ",\"start_qpc\":" << receipt.start << ",\"end_qpc\":" << receipt.end
              << ",\"failure\":\"" << renderer_json_message(receipt.failure) << '"'
              << ",\"launcher_pid\":" << receipt.launcher_pid << ",\"renderer_pid\":" << receipt.renderer_pid
              << ",\"actor_pid\":" << receipt.actor_pid << ",\"console_processes\":" << receipt.associated_consoles
              << ",\"actor_registered_qpc\":" << receipt.registered
              << ",\"columns\":" << receipt.columns << ",\"rows\":" << receipt.rows
              << ",\"control_bytes\":" << receipt.control_bytes
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
              << ",\"pipe_quiescent\":" << truth(receipt.pipe_quiescent) << "},\"phases\":[";
    bool first = true;
    for (const auto& phase : receipt.phases) {
        if (!first) std::cout << ',';
        first = false;
        const auto& pixel = phase.pixels;
        std::cout << "{\"index\":" << phase.index
                  << ",\"name\":\"" << renderer_stage_names[phase.index] << '"'
                  << ",\"expected_visible\":" << truth(renderer_visible[phase.index])
                  << ",\"expected_main\":" << truth(renderer_main[phase.index])
                  << ",\"native_main\":" << truth(phase.native_main)
                  << ",\"native_written\":" << phase.written
                  << ",\"native_marker_read\":" << phase.marker_read
                  << ",\"native_sentinel_read\":" << phase.sentinel_read
                  << ",\"native_input\":" << phase.input << ",\"native_output\":" << phase.output
                  << ",\"actor_qpc\":" << phase.actor_qpc
                  << ",\"capture_qpc\":" << phase.capture_qpc << ",\"ack_qpc\":" << phase.ack_qpc
                  << ",\"crop\":\"phase-" << phase.index << ".bmp\""
                  << ",\"pixels\":{\"client_width\":" << pixel.client_width
                  << ",\"client_height\":" << pixel.client_height
                  << ",\"cell_width\":" << pixel.cell_width
                  << ",\"cell_height\":" << pixel.cell_height
                  << ",\"crop_width\":" << pixel.crop_width
                  << ",\"crop_height\":" << pixel.crop_height
                  << ",\"anchor\":" << pixel.anchor_pixels
                  << ",\"phase\":" << pixel.phase_pixels
                  << ",\"sentinel\":" << pixel.sentinel_pixels
                  << ",\"cursor\":" << pixel.cursor_pixels
                  << ",\"cursor_area\":" << pixel.cursor_area
                  << ",\"stable_frames\":" << pixel.stable_frames << "}}";
    }
    std::cout << "]}\n";
}

int renderer_supervise(const std::wstring& terminal, const std::wstring& work, const std::wstring& nonce) {
    RendererReceipt receipt;
    receipt.start = ticks();
    const auto start = Clock::now();
    const auto deadline = start + 30s;
    const auto work_deadline = start + 25s;
    std::unique_ptr<Server> server;
    Handle job, actor;
    Child launcher;
    RendererWindow renderer;
    bool observed_all = false;
    try {
        require_renderer_nonce(nonce);
        const auto folder = std::filesystem::path(work);
        const auto executable = std::filesystem::path(terminal);
        require(folder.is_absolute() && std::filesystem::is_directory(folder),
                "renderer probe requires a fresh absolute owned work directory");
        require(executable.is_absolute() && same_path(executable.filename().wstring(), L"WindowsTerminal.exe")
                && std::filesystem::is_regular_file(executable.parent_path() / L".portable")
                && std::filesystem::is_regular_file(executable.parent_path() / L"settings" / L"settings.json"),
                "renderer probe requires the isolated portable distribution");
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
        launcher = create(terminal, {L"-w", L"new", L"new-tab", L"--title", title,
                                    L"--suppressApplicationTitle", L"--inheritEnvironment",
                                    L"--", executable_self, L"--renderer-actor", pipe, nonce},
                          work, &startup, 0);
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
            Handle sender(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE, FALSE, pid));
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
                const auto data = renderer_numbers<7>(parts);
                receipt.associated_consoles = data[0];
                receipt.initial_input = data[1];
                receipt.initial_output = data[2];
                receipt.columns = data[3];
                receipt.rows = data[4];
                receipt.registered = renderer_stamp(data[5], data[6]);
                require(receipt.associated_consoles >= 1 && receipt.associated_consoles <= 128
                        && receipt.columns >= 56 && receipt.columns <= 200
                        && receipt.rows >= 10 && receipt.rows <= 80
                        && receipt.registered >= resumed && receipt.registered <= ticks(),
                        "renderer actor registration and viewport bounds");
                receipt.actor_pid = pid;
                actor = std::move(sender);
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

            if (parts[0] == "V") {
                const auto data = renderer_numbers<9>(parts);
                const auto index = receipt.phases.size();
                require(index < renderer_stage_count && data[0] == index, "strict renderer phase ordering");
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
                        && phase.input == receipt.initial_input
                        && phase.output == (receipt.initial_output | ENABLE_PROCESSED_OUTPUT | ENABLE_VIRTUAL_TERMINAL_PROCESSING)
                        && phase.actor_qpc >= prior_qpc && phase.actor_qpc <= ticks(),
                        "renderer native write, marker, buffer and mode evidence");

                if (!renderer.hwnd) {
                    renderer = find_renderer_window(job.value, terminal, title, work_deadline);
                    receipt.renderer_pid = renderer.pid;
                }
                verify_renderer_window(renderer, job.value, terminal, title);
                phase.pixels = observe_renderer_frame(renderer.hwnd, index, renderer_visible[index],
                                                       renderer_main[index], work_deadline,
                                                       folder / (L"phase-" + std::to_wstring(index) + L".bmp"));
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
            require(parts[0] == "F" && receipt.phases.size() == renderer_stage_count,
                    "renderer final restoration follows all measured phases");
            const auto data = renderer_numbers<4>(parts);
            receipt.restored_input = data[0];
            receipt.restored_output = data[1];
            receipt.restore_qpc = renderer_stamp(data[2], data[3]);
            receipt.modes_restored = receipt.restored_input == receipt.initial_input
                && receipt.restored_output == receipt.initial_output;
            require(receipt.modes_restored && receipt.restore_qpc >= prior_qpc && receipt.restore_qpc <= ticks(),
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
        && receipt.job_empty && receipt.pipe_quiescent;
    emit_renderer_receipt(receipt);
    return receipt.passed ? 0 : 1;
}
