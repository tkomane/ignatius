// Test-only retained-grid timing over the already authenticated archive console.
enum class WorkbenchGridOrder { server, ascending, descending };
constexpr std::array<int, 3> renderer_grid_filter_digits{9, 17, 19};

struct WorkbenchGridHeader {
    std::size_t row{}, code{}, key{}, bucket{}, wide{}, results_row{};
};

std::optional<WorkbenchGridHeader> workbench_grid_header(const WorkbenchLines& lines) {
    if (lines.size() != 27 || lines.front().size() != 90) return std::nullopt;
    const auto results = std::find_if(lines.begin(), lines.end(), [](const auto& line) {
        return line.find(L"Results") != std::wstring::npos
            && line.find(L"Completed") != std::wstring::npos
            && line.find(L"row cap reached") != std::wstring::npos;
    });
    if (results == lines.end()) return std::nullopt;
    for (auto row = std::next(results); row != lines.end(); ++row) {
        const auto code = row->find(L"row_code");
        const auto key = row->find(L"sort_key");
        const auto bucket = row->find(L"bucket");
        const auto wide = row->find(L"wide_a");
        if (code < key && key < bucket && bucket < wide && wide != std::wstring::npos) {
            return WorkbenchGridHeader{static_cast<std::size_t>(row - lines.begin()),
                code, key, bucket, wide, static_cast<std::size_t>(results - lines.begin())};
        }
    }
    return std::nullopt;
}

std::optional<unsigned> workbench_grid_number(std::wstring_view field, std::wstring_view padding) {
    const auto first = field.find_first_not_of(padding);
    if (first == std::wstring_view::npos) return std::nullopt;
    field = field.substr(first, field.find_last_not_of(padding) - first + 1);
    if (field.size() > 4 || field.find_first_not_of(L"0123456789") != std::wstring_view::npos) {
        return std::nullopt;
    }
    unsigned value = 0;
    for (const auto ch : field) value = 10 * value + static_cast<unsigned>(ch - L'0');
    return value;
}

std::optional<unsigned> workbench_grid_range_number(std::wstring_view field) {
    std::wstring digits(field);
    digits.erase(std::remove(digits.begin(), digits.end(), L','), digits.end());
    return workbench_grid_number(digits, L"");
}

std::optional<std::pair<unsigned, unsigned>> workbench_grid_range(
        const WorkbenchLines& lines, const WorkbenchGridHeader& header) {
    for (std::size_t row = 0; row < header.row; ++row) {
        const std::wstring_view line(lines[row]);
        const auto prefix = line.find(L"rows ");
        if (prefix == std::wstring_view::npos) continue;
        const auto separator = line.find(L'-', prefix + 5);
        if (separator == std::wstring_view::npos) continue;
        const auto end = line.find(L' ', separator + 1);
        if (end == std::wstring_view::npos) continue;
        const auto first = workbench_grid_range_number(line.substr(prefix + 5, separator - prefix - 5));
        const auto last = workbench_grid_range_number(line.substr(separator + 1, end - separator - 1));
        if (first && last && *first > 0 && *first <= *last) return std::pair{*first, *last};
    }
    return std::nullopt;
}

bool workbench_grid_state(const WorkbenchLines& lines, WorkbenchGridOrder order, bool filtered) {
    const auto header = workbench_grid_header(lines);
    const auto filter_state = filtered
        ? workbench_contains(lines, L"matching 100 of 1,000 retained")
        : !workbench_contains(lines, L" matching ");
    if (!header || !filter_state
        || !workbench_contains(lines, L"retained 1000 of 1050 received rows; 6 source columns.")) {
        return false;
    }

    const auto range = workbench_grid_range(lines, *header);
    const unsigned total = filtered ? 100 : 1000;
    if (!range || range->second > total) return false;

    unsigned rows{};
    for (std::size_t y = header->row + 1; y < lines.size(); ++y) {
        const std::wstring_view line(lines[y]);
        if (line.substr(header->code, 3) != L"row") continue;
        constexpr std::wstring_view trim_columns = L" |\u2502\u2503";
        const auto gutter = workbench_grid_number(line.substr(0, header->code), trim_columns);
        const auto id = workbench_grid_number(line.substr(header->code + 3, 4), L"");
        const auto rank = workbench_grid_number(line.substr(header->key, header->bucket - header->key), trim_columns);
        if (!gutter || !id || *id < 1 || *id > 1000 || *gutter != *id
            || !rank || *rank != (613 * *id) % 1000) {
            return false;
        }
        const auto bucket = line.substr(header->bucket, header->wide - header->bucket);
        const bool match = *id % 10 == 1;
        if ((bucket.find(match ? L"hit#" : L"other") == std::wstring_view::npos) || (filtered && !match)) {
            return false;
        }
        const auto wide = line.substr(header->wide);
        if (std::count(wide.begin(), wide.end(), L'\u754c') < 12
            || std::count(wide.begin(), wide.end(), L'\u8868') < 12) {
            return false;
        }

        // Verify the entire visible range, including every slot promised by the
        // toolbar. A half-painted frame can retain individually valid rows.
        const auto position = range->first - 1 + rows;
        if (position >= range->second) return false;
        if (order == WorkbenchGridOrder::server) {
            if (*id != (filtered ? 10 * position + 1 : position + 1)) return false;
        } else {
            const auto ordered = order == WorkbenchGridOrder::ascending ? position : total - 1 - position;
            if (*rank != (filtered ? 10 * ordered + 3 : ordered)) return false;
        }
        ++rows;
    }
    return rows >= 2 && rows == range->second - range->first + 1;
}

std::vector<INPUT_RECORD> workbench_grid_key(wchar_t character, bool chord = false) {
    std::vector<INPUT_RECORD> records;
    if (chord) renderer_add_key(records, 'K', L'k', LEFT_CTRL_PRESSED);
    const SHORT mapped = VkKeyScanW(character);
    require(mapped != -1 && HIBYTE(mapped) <= 1, "grid timing uses one normal key on the fixture keyboard");
    renderer_add_key(records, LOBYTE(mapped), character, HIBYTE(mapped) == 1 ? SHIFT_PRESSED : 0);
    return records;
}

void renderer_workbench_grid(const Child& client, HANDLE input, Clock::time_point archive_deadline,
                             const std::wstring& work, const std::wstring& pipe, const std::wstring& nonce) {
    const auto work_deadline = archive_deadline - 6s;
    require(Clock::now() < work_deadline, "leave six seconds for the existing workbench query and quit");
    const auto editor_focused = [](const WorkbenchLines& lines) {
        return std::any_of(lines.begin(), lines.end(), [](const auto& line) {
            return line.find(L"Editor") != std::wstring::npos && line.find(L"[focused]") != std::wstring::npos;
        });
    };
    const auto connected_catalogue = [](const WorkbenchLines& lines) {
        return workbench_contains(lines, L"Ready Autocommit") && workbench_contains(lines, L"perf_aux")
            && workbench_contains(lines, L"tables (15)");
    };
    workbench_observe_until(client, work_deadline, connected_catalogue, "connected synthetic grid catalogue");

    // Keep sufficient columns visible in the existing 90x27 window.
    renderer_key(input, 'B', L'b', LEFT_CTRL_PRESSED);
    workbench_observe_until(client, work_deadline, [&](const auto& lines) {
        return editor_focused(lines) && !workbench_contains(lines, L"perf_aux");
    }, "hide schema sidebar before the grid query");
    renderer_send_keys(input, workbench_grid_key(L'=', true));
    workbench_observe_until(client, work_deadline, [&](const auto& lines) {
        return editor_focused(lines) && workbench_contains(lines, L"Editor  line 1, column 1 ");
    }, "fresh grid query buffer");
    constexpr std::wstring_view select = L"SELECT row_code, sort_key, bucket, wide_a, wide_b, nullable";
    constexpr std::wstring_view from = L" FROM public.perf_grid ORDER BY source_id;";
    renderer_type_text(input, select);
    renderer_type_text(input, from);
    const auto caret = L"Editor  line 1, column " + std::to_wstring(select.size() + from.size() + 1) + L" ";
    workbench_observe_until(client, work_deadline, [&](const auto& lines) {
        return editor_focused(lines) && workbench_contains(lines, caret)
            && workbench_contains(lines, L"FROM public.perf_grid ORDER BY source_id;");
    }, "complete grid query before execution");
    renderer_key(input, 'R', L'r', LEFT_CTRL_PRESSED);

    constexpr auto server = WorkbenchGridOrder::server;
    constexpr auto ascending = WorkbenchGridOrder::ascending;
    constexpr auto descending = WorkbenchGridOrder::descending;
    const auto state = [](WorkbenchGridOrder order, bool filtered = false) {
        return [=](const WorkbenchLines& lines) { return workbench_grid_state(lines, order, filtered); };
    };
    workbench_observe_until(client, work_deadline, state(server), "exact retained grid with visible Unicode");
    renderer_key(input, VK_TAB, L'\t');
    workbench_observe_until(client, work_deadline, [](const auto& lines) {
        return workbench_contains(lines, L"Results  Completed  row cap reached [focused]");
    }, "focus the retained Results grid");
    renderer_key(input, VK_RIGHT, L'\0');
    const auto palette = [](const WorkbenchLines& lines) {
        return workbench_contains(lines, L"Result grid  Enter applies")
            && workbench_contains(lines, L"Sort selected column")
            && workbench_contains(lines, L"selected column 2: sort_key");
    };
    const auto open_sort = [&](Clock::time_point until) {
        (void)workbench_observe(client);
        renderer_send_keys(input, workbench_grid_key(L'g', true));
        workbench_observe_until(client, until, palette, "open the controls for sort_key");
    };
    open_sort(work_deadline);
    renderer_key(input, VK_ESCAPE, L'\x1b');
    workbench_observe_until(client, work_deadline, state(server), "restore the complete grid after column admission");

    const auto deadline = (std::min)(work_deadline, Clock::now() + 12s);
    const auto idle = workbench_observe(client);
    const auto baseline = workbench_grid_header(idle);
    require(baseline.has_value() && workbench_grid_state(idle, server, false),
            "stable grid header and original row order before timing");
    require(remaining(deadline, 100) == 100, "time remains for the quiet grid control");
    Sleep(100);
    require(workbench_observe(client) == idle, "unchanged grid text during the idle control");

    constexpr std::size_t warmup = 3;
    constexpr std::size_t cycles = 15;
    constexpr std::array<const char*, 7> names{
        "sort_ascending", "sort_descending", "sort_server", "filter_apply",
        "filter_clear", "column_widen", "column_narrow"
    };
    std::array<std::vector<std::uint64_t>, names.size()> samples;

    const auto layout = [&](std::size_t extra) {
        return [&, extra](const WorkbenchLines& lines) {
            const auto observed = workbench_grid_header(lines);
            return observed && observed->code == baseline->code && observed->key == baseline->key
                && observed->bucket == baseline->bucket + extra && observed->wide == baseline->wide + extra
                && workbench_grid_state(lines, ascending, false);
        };
    };
    const auto hash_key = workbench_grid_key(L'#');
    const auto widen_key = workbench_grid_key(L']', true);
    const auto narrow_key = workbench_grid_key(L'[', true);
    for (std::size_t cycle = 0; cycle < warmup + cycles; ++cycle) {
        const auto retain = [&](std::size_t slot, std::uint64_t value) {
            if (cycle >= warmup) samples[slot].push_back(value);
        };
        require(workbench_grid_state(workbench_observe(client), server, false), "grid cycle starts in server order");
        open_sort(deadline);
        retain(0, workbench_time_key(client, input, deadline, VK_RETURN, L'\r', 0, state(ascending), names[0]));

        (void)workbench_observe(client);
        renderer_send_keys(input, workbench_grid_key(L'f', true));
        workbench_observe_until(client, deadline, [](const auto& lines) {
            return workbench_contains(lines, L"[filter:");
        }, "open the normal result filter");
        retain(3, workbench_time_input(client, input, deadline, hash_key, state(ascending, true), names[3]));
        retain(4, workbench_time_key(client, input, deadline, VK_ESCAPE, L'\x1b', 0, [&](const auto& lines) {
            return !workbench_contains(lines, L"[filter:") && workbench_grid_state(lines, ascending, false);
        }, names[4]));

        require(layout(0)(workbench_observe(client)), "original column geometry before widening");
        retain(5, workbench_time_input(client, input, deadline, widen_key, layout(2), names[5]));
        require(layout(2)(workbench_observe(client)), "expanded column geometry before narrowing");
        retain(6, workbench_time_input(client, input, deadline, narrow_key, layout(0), names[6]));

        open_sort(deadline);
        retain(1, workbench_time_key(client, input, deadline, VK_RETURN, L'\r', 0, state(descending), names[1]));
        open_sort(deadline);
        retain(2, workbench_time_key(client, input, deadline, VK_RETURN, L'\r', 0, state(server), names[2]));
    }

    // Keep the exact filtered grid displayed until the authenticated supervisor
    // acknowledges one post-input WGC frame. No witness work enters the samples.
    const auto unfiltered = workbench_observe(client);
    const auto witness_deadline = (std::min)(work_deadline, Clock::now() + 3s);
    renderer_send_keys(input, workbench_grid_key(L'f', true));
    workbench_observe_until(client, witness_deadline, [](const auto& lines) {
        return workbench_contains(lines, L"[filter:");
    }, "open the filter for the retained grid witness");
    renderer_send_keys(input, hash_key);
    workbench_observe_until(client, witness_deadline, state(server, true), "exact filtered grid witness");
    const auto filtered = workbench_observe(client);
    const auto heading = workbench_grid_header(filtered);
    require(heading && workbench_grid_state(filtered, server, true), "coherent native grid before its WGC witness");
    static constexpr std::wstring_view matching = L"matching 100 of 1000 retained rows";
    const auto status = std::find_if(filtered.begin() + static_cast<std::ptrdiff_t>(heading->results_row),
        filtered.end(), [](const auto& line) { return line.find(matching) != std::wstring::npos; });
    const auto first = std::find_if(filtered.begin() + static_cast<std::ptrdiff_t>(heading->row + 1),
        filtered.end(), [&](const auto& line) { return line.substr(heading->code, 3) == L"row"; });
    require(status != filtered.end() && first != filtered.end() && std::next(first) != filtered.end()
        && std::next(first)->substr(heading->code, 3) == L"row", "fixed status and data cells for the grid witness");
    const auto status_row = static_cast<std::size_t>(status - filtered.begin());
    const auto status_column = status->find(matching);
    for (const int digit : renderer_grid_filter_digits) {
        require(unfiltered.at(status_row).at(status_column + static_cast<std::size_t>(digit)) == L' ',
                "the unfiltered grid lacks the three matching-count digit anchors");
    }
    std::ostringstream witness;
    witness << "G " << ascii(nonce) << ' ' << heading->results_row << ' ' << status_row
            << ' ' << heading->row << ' ' << (first - filtered.begin()) << ' ' << status_column
            << ' ' << heading->code << ' ' << heading->wide;
    renderer_stamp(witness, ticks());
    send(pipe, witness.str());
    renderer_key(input, VK_ESCAPE, L'\x1b');
    workbench_observe_until(client, work_deadline, state(server), "clear the witness filter before TLS proof");

    renderer_key(input, VK_TAB, L'\t');
    workbench_observe_until(client, work_deadline, editor_focused, "restore the focused Editor before TLS proof");
    renderer_key(input, 'B', L'b', LEFT_CTRL_PRESSED);
    workbench_observe_until(client, work_deadline, connected_catalogue, "restore the workbench sidebar before TLS proof");

    std::ofstream receipt(std::filesystem::path(work) / L"workbench-performance.pending");
    receipt << "{\"schema_version\":1,\"scope\":\"native retained grid console screen-buffer timing\","
            << "\"warmup_actions\":" << warmup * names.size()
            << ",\"measured_actions\":" << cycles * names.size()
            << ",\"idle_unchanged\":true,\"catalogue_loaded\":true,\"qpc_frequency\":" << frequency()
            << ",\"viewport_columns\":90,\"viewport_rows\":27,\"rows_retained\":1000,\"rows_returned\":1050,"
            << "\"source_columns\":6,\"filter_matches\":100,\"wide_glyphs_verified\":true,"
            << "\"source_row_numbers_verified\":true,\"sort_column_width_step\":2,\"samples\":{";
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
    require(static_cast<bool>(receipt), "write the bounded owned grid timing receipt");
}
