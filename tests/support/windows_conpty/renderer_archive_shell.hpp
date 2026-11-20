// Live archive scenario inside the already owned native ARM64 Terminal.
struct RendererShellConfig {
    std::wstring executable;
    unsigned port{};
};

std::optional<RendererShellConfig> renderer_shell_config(const std::filesystem::path& work) {
    const auto request = work / L"shell-request";
    if (!std::filesystem::exists(request)) return std::nullopt;
    require(std::filesystem::is_regular_file(request) && std::filesystem::file_size(request) <= 540,
            "bounded PowerShell workbench request");
    std::ifstream data(request);
    std::string path, extra;
    unsigned port{};
    require(static_cast<bool>(std::getline(data, path)) && static_cast<bool>(data >> port)
            && !(data >> extra) && port > 0 && port <= 65535,
            "read the scoped PowerShell path and PostgreSQL port");
    const auto exe = std::filesystem::path(wide_ascii(path));
    require(exe.is_absolute() && same_path(exe.filename().wstring(), L"pwsh.exe")
            && std::filesystem::is_regular_file(exe)
            && std::filesystem::is_regular_file(work / L"Workbench-Shell.ps1"),
            "the PowerShell scenario uses the identified executable and owned script");
    return RendererShellConfig{exe.wstring(), port};
}

void renderer_report_process(const std::wstring& pipe, const std::wstring& nonce, char stage, const Child& child) {
    const auto created = birth(child.process.value);
    std::ostringstream registration;
    registration << stage << ' ' << ascii(nonce) << ' ' << child.pid;
    renderer_stamp(registration, static_cast<std::uint64_t>(reinterpret_cast<std::uintptr_t>(child.process.value)));
    registration << ' ' << created.dwLowDateTime << ' ' << created.dwHighDateTime;
    send(pipe, registration.str());
}

void renderer_shell_signal(const std::filesystem::path& work, const wchar_t* name) {
    std::ofstream marker(work / name);
    marker.put('1');
    marker.close();
    require(static_cast<bool>(marker), "write the owned shell handshake marker");
}

std::ifstream renderer_shell_reply(const std::filesystem::path& work, const wchar_t* name,
                                    HANDLE shell, Clock::time_point deadline) {
    const auto marker = work / name;
    while (Clock::now() < deadline) {
        require(WaitForSingleObject(shell, 0) == WAIT_TIMEOUT, "owned PowerShell exited during its handshake");
        if (std::filesystem::is_regular_file(marker)) {
            require(std::filesystem::file_size(marker) <= 64, "bounded owned PowerShell reply");
            return std::ifstream(marker);
        }
        Sleep(remaining(deadline, 20));
    }
    throw std::runtime_error("owned PowerShell did not publish its handshake");
}

Child renderer_start_shell(const RendererShellConfig& config, const std::wstring& archive,
                           const std::wstring& pipe, const std::wstring& nonce, const std::wstring& work) {
    STARTUPINFOW startup{};
    startup.cb = sizeof(startup);
    const auto folder = std::filesystem::path(work);
    const auto script = folder / L"Workbench-Shell.ps1";
    const auto profile = folder / L"portable-profile";
    const std::vector<std::wstring> shell_environment{
        L"USERPROFILE=" + profile.wstring(),
        L"APPDATA=" + (profile / L"AppData/Roaming").wstring(),
        L"LOCALAPPDATA=" + (profile / L"AppData/Local").wstring()
    };
    auto shell = create(config.executable, {L"-NoLogo", L"-NoProfile", L"-NoExit", L"-File", script.wstring(),
                        L"-Archive", archive, L"-Port", std::to_wstring(config.port)}, work, &startup, 0,
                        shell_environment);
    renderer_report_process(pipe, nonce, 'H', shell);
    resume(shell);
    auto ready = renderer_shell_reply(work, L"shell-ready", shell.process.value, Clock::now() + 7s);
    std::string version;
    require(static_cast<bool>(ready >> version) && version.compare(0, 2, "7.") == 0,
            "the owned PowerShell process reports major version 7");
    return shell;
}

Child renderer_start_archive_from_shell(const std::wstring& work, HANDLE shell, const std::wstring& archive,
                                       Clock::time_point deadline) {
    renderer_shell_signal(work, L"shell-launch");
    auto receipt = renderer_shell_reply(work, L"shell-client", shell, deadline);
    DWORD pid{};
    std::uintptr_t inherited{};
    std::string extra;
    require(static_cast<bool>(receipt >> pid >> inherited) && !(receipt >> extra) && pid != 0 && inherited != 0,
            "owned PowerShell published one native product handle");
    HANDLE raw{};
    require(DuplicateHandle(shell, reinterpret_cast<HANDLE>(inherited), GetCurrentProcess(), &raw,
                PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE, FALSE, 0) != 0,
            "duplicate the archived process handle directly from the owned shell");
    Child product{Handle(raw), {}, pid};
    require(GetProcessId(product.process.value) == pid && same_path(image(product.process.value), archive)
            && WaitForSingleObject(product.process.value, 0) == WAIT_TIMEOUT,
            "the owned shell launched the exact retained archive");
    return product;
}

void renderer_send_keys(HANDLE input, const std::vector<INPUT_RECORD>& keys) {
    DWORD written{};
    require(WriteConsoleInputW(input, keys.data(), static_cast<DWORD>(keys.size()), &written) != 0
            && written == keys.size(), "send bounded keys to the owned native console");
}

void renderer_add_key(std::vector<INPUT_RECORD>& keys, WORD virtual_key, wchar_t character, DWORD state = 0) {
    INPUT_RECORD record{};
    record.EventType = KEY_EVENT;
    auto& key = record.Event.KeyEvent;
    key.bKeyDown = TRUE;
    key.wRepeatCount = 1;
    key.wVirtualKeyCode = virtual_key;
    key.wVirtualScanCode = static_cast<WORD>(MapVirtualKeyW(virtual_key, MAPVK_VK_TO_VSC));
    key.uChar.UnicodeChar = character;
    key.dwControlKeyState = state;
    keys.push_back(record);
    key.bKeyDown = FALSE;
    keys.push_back(record);
}

void renderer_type_text(HANDLE input, std::wstring_view text) {
    require(!text.empty() && text.size() <= 80, "bounded synthetic console command");
    std::vector<INPUT_RECORD> keys;
    for (const wchar_t ch : text) {
        const SHORT mapped = VkKeyScanW(ch);
        require(mapped != -1 && HIBYTE(mapped) <= 1, "synthetic key uses the current Latin layout");
        renderer_add_key(keys, LOBYTE(mapped), ch, HIBYTE(mapped) == 1 ? SHIFT_PRESSED : 0);
    }
    renderer_send_keys(input, keys);
}

void renderer_key(HANDLE input, WORD virtual_key, wchar_t character, DWORD state = 0) {
    std::vector<INPUT_RECORD> keys;
    renderer_add_key(keys, virtual_key, character, state);
    renderer_send_keys(input, keys);
}

std::vector<std::wstring> renderer_console_lines() {
    const auto output = renderer_open_output();
    CONSOLE_SCREEN_BUFFER_INFO sample{};
    require(GetConsoleScreenBufferInfo(output.value, &sample) != 0, "read owned workbench viewport");
    const auto viewport = sample.srWindow;
    const auto width = viewport.Right - viewport.Left + 1;
    const auto height = viewport.Bottom - viewport.Top + 1;
    require(width >= 80 && width <= 200 && height >= 24 && height <= 80,
            "bounded full-screen workbench observation");
    std::vector<std::wstring> lines;
    for (SHORT row = viewport.Top; row <= viewport.Bottom; ++row) {
        std::wstring line(static_cast<std::size_t>(width), L'\0');
        renderer_read(output.value, {viewport.Left, row}, line);
        lines.push_back(std::move(line));
    }
    return lines;
}

template<class Condition>
void renderer_wait_for_screen(const Child& process, Clock::time_point deadline, Condition condition,
                              const std::wstring& work, const char* failure) {
    std::vector<std::wstring> lines;
    while (Clock::now() < deadline) {
        require(WaitForSingleObject(process.process.value, 0) == WAIT_TIMEOUT
                && renderer_product_attached(process.pid), "only the owned attached process receives diagnostic input");
        lines = renderer_console_lines();
        if (condition(lines)) return;
        Sleep(remaining(deadline, 30));
    }
    // Retain only the last bounded synthetic screen from the owned console.
    std::ofstream snapshot(std::filesystem::path(work) / L"workbench-failure.txt");
    snapshot << failure << '\n';
    for (const auto& line : lines) {
        for (const wchar_t ch : line) {
            snapshot.put(ch >= L' ' && ch <= L'~' ? static_cast<char>(ch) : '?');
        }
        snapshot.put('\n');
    }
    throw std::runtime_error(failure);
}

void renderer_shell_stage(const std::wstring& pipe, const std::wstring& nonce, unsigned stage,
                          COORD cell = {}) {
    std::ostringstream line;
    line << "W " << ascii(nonce) << ' ' << stage << ' ' << cell.X << ' ' << cell.Y;
    renderer_stamp(line, ticks());
    send(pipe, line.str());
}

COORD renderer_workbench_query(const Child& product, HANDLE input, Clock::time_point deadline,
                               const std::wstring& work) {
    renderer_wait_for_screen(product, deadline, [](const auto& lines) {
        return lines.front().find(L"ig_probe@localhost:") != std::wstring::npos
            && std::any_of(lines.begin(), lines.end(), [](const auto& line) {
                return line.find(L"Editor") != std::wstring::npos && line.find(L"[focused]") != std::wstring::npos;
            });
    }, work, "the archived workbench did not connect with editor focus");

    renderer_key(input, 'K', L'k', LEFT_CTRL_PRESSED);
    renderer_type_text(input, L"=");
    renderer_wait_for_screen(product, deadline, [](const auto& lines) {
        return std::any_of(lines.begin(), lines.end(), [](const auto& line) {
            return line.find(L"Editor  line 1, column 1 ") != std::wstring::npos
                && line.find(L"[focused]") != std::wstring::npos;
        });
    }, work, "the archived workbench did not open a blank query buffer");
    constexpr std::wstring_view select = L"select 42 as journey_value from pg_catalog.pg_stat_ssl";
    constexpr std::wstring_view tls = L" where pid=pg_backend_pid() and ssl";
    constexpr std::wstring_view read_only = L" and current_setting('transaction_read_only')::bool";
    renderer_type_text(input, select);
    renderer_type_text(input, tls);
    renderer_type_text(input, read_only);
    const auto caret = L"Editor  line 1, column " + std::to_wstring(select.size() + tls.size() + read_only.size() + 1) + L" ";
    renderer_wait_for_screen(product, deadline, [&caret](const auto& lines) {
        return std::any_of(lines.begin(), lines.end(), [&caret](const auto& line) {
            return line.find(caret) != std::wstring::npos && line.find(L"[modified] [focused]") != std::wstring::npos;
        });
    }, work, "the archived workbench did not render the completed query before execution");
    renderer_key(input, 'R', L'r', LEFT_CTRL_PRESSED);
    COORD result_position{};
    renderer_wait_for_screen(product, deadline, [&result_position](const auto& lines) {
        constexpr std::wstring_view name = L"journey_value";
        const auto caption = std::find_if(lines.begin(), lines.end(), [](const auto& line) {
            return line.find(L"Results") != std::wstring::npos && line.find(L"Completed") != std::wstring::npos
                && line.find(L"1 row, ") != std::wstring::npos && line.find(L" ms") != std::wstring::npos;
        });
        if (caption == lines.end()) return false;
        for (auto header = caption + 1; lines.end() - header >= 3; ++header) {
            const auto start = header->find(name);
            if (start == std::wstring::npos) continue;
            const auto is_rule = [&](const auto& line) {
                return std::wstring_view(line).substr(start, name.size())
                    .find_first_not_of(L"-\u2500") == std::wstring_view::npos;
            };
            auto rule = header + 1;
            if (!is_rule(*rule)) ++rule;  // The optional type row precedes the rule.
            if (rule + 1 == lines.end() || !is_rule(*rule)) continue;
            const auto cell = std::wstring_view(*(rule + 1)).substr(start, name.size());
            const auto first = cell.find_first_not_of(L' ');
            if (first == std::wstring_view::npos || cell.substr(first, 2) != L"42"
                || cell.find_last_not_of(L' ') != first + 1) return false;
            result_position = {static_cast<SHORT>(start + first),
                               static_cast<SHORT>((rule + 1) - lines.begin())};
            return true;
        }
        return false;
    }, work, "the archived workbench did not display the completed PostgreSQL result");
    return result_position;
}

void renderer_use_restored_shell(const Child& shell, const std::wstring& work, HANDLE input,
                                 Clock::time_point deadline, const std::wstring& pipe, const std::wstring& nonce) {
    renderer_shell_signal(work, L"shell-return");
    const auto contains = [](std::wstring_view token) {
        return [token](const auto& lines) {
            return std::any_of(lines.begin(), lines.end(),
                [token](const auto& line) { return line.find(token) != std::wstring::npos; });
        };
    };
    renderer_wait_for_screen(shell, deadline, contains(L"W07-PS> "), work, "PowerShell did not restore its interactive prompt");
    renderer_type_text(input, L"Write-Output ('IG_'+'SHELL_RETURNED')");
    renderer_key(input, VK_RETURN, L'\r');
    renderer_wait_for_screen(shell, deadline, contains(L"IG_SHELL_RETURNED"), work, "the same PowerShell did not execute its command");
    renderer_shell_stage(pipe, nonce, 2);
    renderer_type_text(input, L"exit 0");
    renderer_key(input, VK_RETURN, L'\r');
    DWORD exit_code{};
    require(WaitForSingleObject(shell.process.value, remaining(deadline, 20000)) == WAIT_OBJECT_0
            && GetExitCodeProcess(shell.process.value, &exit_code) && exit_code == 0,
            "wait for the same owned PowerShell to exit zero");
}
