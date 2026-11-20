// Test-only console-state timings for the archived PowerShell workbench.
using WorkbenchLines = std::vector<std::wstring>;

bool workbench_contains(const WorkbenchLines& lines, std::wstring_view text) {
    return std::any_of(lines.begin(), lines.end(), [text](const auto& line) {
        return line.find(text) != std::wstring::npos;
    });
}

WorkbenchLines workbench_observe(const Child& client) {
    require(WaitForSingleObject(client.process.value, 0) == WAIT_TIMEOUT
            && renderer_product_attached(client.pid), "performance probe still owns the attached archive");
    return renderer_console_lines();
}

template<class Predicate>
std::uint64_t workbench_observe_until(const Child& client, Clock::time_point deadline,
                                      Predicate expected, const char* action) {
    while (Clock::now() < deadline) {
        const auto frame = workbench_observe(client);
        const auto observed = ticks();
        if (Clock::now() < deadline && expected(frame)) return observed;
        Sleep(remaining(deadline, 4));
    }
    throw std::runtime_error(std::string("workbench console observation timed out: ") + action);
}

template<class Predicate>
std::uint64_t workbench_time_key(const Child& client, HANDLE input, Clock::time_point deadline,
                                 WORD key, wchar_t character, DWORD modifiers,
                                 Predicate expected, const char* action) {
    if (expected(workbench_observe(client))) {
        throw std::runtime_error(std::string("workbench state already satisfied: ") + action);
    }
    std::vector<INPUT_RECORD> records;
    renderer_add_key(records, key, character, modifiers);
    require(Clock::now() < deadline, "workbench timing input is within its budget");
    const auto sample_deadline = (std::min)(deadline, Clock::now() + 1s);
    const auto start = ticks();
    renderer_send_keys(input, records);
    const auto end = workbench_observe_until(client, sample_deadline, expected, action);
    require(end > start, "monotonic workbench action timing");
    return end - start;
}

void renderer_workbench_performance(const Child& client, HANDLE input,
                                    Clock::time_point archive_deadline, const std::wstring& work) {
    const auto deadline = (std::min)(archive_deadline - 6s, Clock::now() + 8s);
    require(Clock::now() < deadline, "leave six seconds for the existing workbench query and quit");
    const std::wstring prefix = L"SELECT * FROM public.perf_catalog_0";
    const auto inserted = prefix + L"X";
    const auto caret = [](std::size_t length) {
        return L"Editor  line 1, column " + std::to_wstring(length + 1) + L" ";
    };
    const auto original_caret = caret(prefix.size());
    const auto inserted_caret = caret(inserted.size());
    const auto completion_title = [](const std::wstring& line) {
        constexpr std::wstring_view title = L" Complete ";
        constexpr std::wstring_view horizontal_border = L"-\u2500\u2501\u2550";
        const auto position = line.find(title);
        return position != std::wstring::npos && position + title.size() < line.size()
            && horizontal_border.find(line[position + title.size()]) != std::wstring::npos;
    };
    const auto completion = [&](const WorkbenchLines& lines) {
        const auto title = std::find_if(lines.begin(), lines.end(), completion_title);
        if (title == lines.end()) return false;
        const auto options_end = title + (std::min)(lines.end() - title, std::ptrdiff_t{6});
        return std::any_of(title + 1, options_end, [](const auto& line) {
            return line.find(L"perf_catalog_01") != std::wstring::npos;
        });
    };
    const auto palette = [](const WorkbenchLines& lines) {
        return workbench_contains(lines, L"Command palette")
            && workbench_contains(lines, L"Type to search")
            && workbench_contains(lines, L"match(es)");
    };
    const auto editor = [&](const WorkbenchLines& lines) {
        return workbench_contains(lines, original_caret) && workbench_contains(lines, prefix)
            && !workbench_contains(lines, inserted)
            && !workbench_contains(lines, L"Type to search")
            && std::none_of(lines.begin(), lines.end(), completion_title);
    };
    const auto changed = [&](const WorkbenchLines& lines) {
        return workbench_contains(lines, inserted_caret) && workbench_contains(lines, inserted);
    };

    (void)workbench_observe(client);
    renderer_key(input, 'K', L'k', LEFT_CTRL_PRESSED);
    renderer_type_text(input, L"=");
    workbench_observe_until(client, deadline, [](const auto& lines) {
        return workbench_contains(lines, L"Editor  line 1, column 1 ")
            && workbench_contains(lines, L"[focused]");
    }, "initial performance editor");
    renderer_type_text(input, prefix);
    workbench_observe_until(client, deadline, editor, "performance editor prefix");
    require(!completion(workbench_observe(client)), "workbench completion starts closed");
    renderer_key(input, VK_F8, L'\0', 0);
    workbench_observe_until(client, (std::min)(deadline, Clock::now() + 3s),
                           completion, "loaded synthetic catalogue");
    (void)workbench_time_key(client, input, deadline, VK_ESCAPE, L'\x1b', 0, editor, "dismiss catalogue warmload");

    const auto idle = workbench_observe(client);
    require(remaining(deadline, 100) == 100, "time remains for the quiet console control");
    Sleep(100);
    require(workbench_observe(client) == idle, "unchanged console text during the idle control");

    constexpr std::size_t warmup = 4;
    constexpr std::size_t cycles = 20;
    constexpr std::array<const char*, 6> names{
        "editor_insert", "editor_backspace", "palette_open", "palette_close",
        "completion_open", "completion_close"
    };
    std::array<std::vector<std::uint64_t>, names.size()> samples;
    for (std::size_t cycle = 0; cycle < warmup + cycles; ++cycle) {
        const auto measure = [&](std::size_t slot, WORD key, wchar_t character, DWORD modifiers, auto expected) {
            const auto sample = workbench_time_key(client, input, deadline, key, character, modifiers,
                                                   expected, names[slot]);
            if (cycle >= warmup) samples[slot].push_back(sample);
        };
        measure(0, 'X', L'X', SHIFT_PRESSED, changed);
        measure(1, VK_BACK, L'\b', 0, editor);
        measure(2, 'P', L'p', LEFT_CTRL_PRESSED, palette);
        measure(3, VK_ESCAPE, L'\x1b', 0, editor);
        measure(4, VK_F8, L'\0', 0, completion);
        measure(5, VK_ESCAPE, L'\x1b', 0, editor);
    }

    // PowerShell retains this receipt only after the original TLS query and
    // Windows Terminal restoration assertions also pass.
    std::ofstream receipt(std::filesystem::path(work) / L"workbench-performance.pending");
    receipt << "{\"schema_version\":1,\"scope\":\"native console screen-buffer timing\","
            << "\"completion_key\":\"F8\","
            << "\"warmup_actions\":" << warmup * names.size()
            << ",\"measured_actions\":" << cycles * names.size()
            << ",\"idle_unchanged\":true,\"catalogue_loaded\":true,\"qpc_frequency\":" << frequency()
            << ",\"viewport_columns\":" << idle.front().size() << ",\"viewport_rows\":" << idle.size()
            << ",\"samples\":{";
    for (std::size_t group = 0; group < names.size(); ++group) {
        if (group != 0) receipt << ',';
        receipt << '"' << names[group] << "\":[";
        for (std::size_t index = 0; index < samples[group].size(); ++index) {
            if (index != 0) receipt << ',';
            receipt << samples[group][index];
        }
        receipt << ']';
    }
    receipt << "}}\n";
    receipt.close();
    require(static_cast<bool>(receipt), "write the bounded owned workbench timing receipt");
}
