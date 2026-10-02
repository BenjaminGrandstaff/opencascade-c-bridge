/*
 * Sessions: ABI version, lifecycle, last error, warnings, diagnostics,
 * options, and handle removal.
 */

#include "bridge_internal.hpp"

#include <algorithm>
#include <cmath>
#include <cstring>

using namespace occt_bridge_internal;

namespace {

size_t copy_text(const std::string& text, char* buffer, size_t buffer_capacity) {
    if (buffer != nullptr && buffer_capacity != 0) {
        const size_t amount = std::min(buffer_capacity - 1, text.size());
        std::memcpy(buffer, text.data(), amount);
        buffer[amount] = '\0';
    }
    return text.size() + 1;
}

}  // namespace

extern "C" {

uint32_t occt_bridge_abi_version(void) {
    return OCCT_BRIDGE_ABI_VERSION;
}

const char* occt_bridge_status_string(occt_bridge_status_t status) {
    switch (status) {
        case OCCT_BRIDGE_OK: return "ok";
        case OCCT_BRIDGE_INVALID_ARGUMENT: return "invalid argument";
        case OCCT_BRIDGE_UNSUPPORTED_ABI: return "unsupported ABI version";
        case OCCT_BRIDGE_SHAPE_NOT_FOUND: return "shape not found";
        case OCCT_BRIDGE_INVALID_GEOMETRY: return "invalid geometry";
        case OCCT_BRIDGE_IO_ERROR: return "I/O error";
        case OCCT_BRIDGE_KERNEL_ERROR: return "Open Cascade kernel error";
        case OCCT_BRIDGE_ALLOCATION_FAILED: return "allocation failed";
        case OCCT_BRIDGE_INTERNAL_ERROR: return "internal error";
        default: return "unknown status";
    }
}

occt_bridge_status_t occt_bridge_session_create(
    uint32_t requested_abi_version,
    occt_bridge_session_t** out_session) {
    if (out_session == nullptr) {
        return OCCT_BRIDGE_INVALID_ARGUMENT;
    }
    *out_session = nullptr;
    if (requested_abi_version != OCCT_BRIDGE_ABI_VERSION) {
        return OCCT_BRIDGE_UNSUPPORTED_ABI;
    }
    try {
        *out_session = new (std::nothrow) occt_bridge_session_t();
        return *out_session == nullptr ? OCCT_BRIDGE_ALLOCATION_FAILED : OCCT_BRIDGE_OK;
    } catch (...) {
        return OCCT_BRIDGE_INTERNAL_ERROR;
    }
}

void occt_bridge_session_destroy(occt_bridge_session_t* session) {
    try {
        delete session;
    } catch (...) {  // NOLINT(bugprone-empty-catch): no exception may cross the C ABI and there is no status to report.
    }
}

occt_bridge_status_t occt_bridge_session_clear(occt_bridge_session_t* session) {
    return guarded(session, [&] {
        session->shapes.clear();
        session->histories.clear();
        return succeed(session);
    });
}

occt_bridge_status_t occt_bridge_session_shape_count(
    occt_bridge_session_t* session,
    size_t* out_count) {
    return guarded(session, [&] {
        if (out_count == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_count is null");
        }
        *out_count = session->shapes.size();
        return succeed(session);
    });
}

size_t occt_bridge_session_last_error(
    const occt_bridge_session_t* session,
    char* buffer,
    size_t buffer_capacity) {
    if (session == nullptr) {
        return 0;
    }
    try {
        std::lock_guard<std::mutex> lock(session->mutex);
        return copy_text(session->last_error, buffer, buffer_capacity);
    } catch (...) {
        return 0;
    }
}

size_t occt_bridge_session_last_warnings(
    const occt_bridge_session_t* session,
    char* buffer,
    size_t buffer_capacity) {
    if (session == nullptr) {
        return 0;
    }
    try {
        std::lock_guard<std::mutex> lock(session->mutex);
        return copy_text(session->last_warnings, buffer, buffer_capacity);
    } catch (...) {
        return 0;
    }
}

occt_bridge_status_t occt_bridge_session_diagnostic_count(
    occt_bridge_session_t* session,
    size_t* out_count) {
    return guarded(
        session,
        [&] {
            if (out_count == nullptr) {
                return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_count is null");
            }
            *out_count = session->last_diagnostics.size();
            return occt_bridge_status_t{OCCT_BRIDGE_OK};
        },
        true);
}

occt_bridge_status_t occt_bridge_session_diagnostic_at(
    occt_bridge_session_t* session,
    size_t index,
    occt_bridge_diagnostic_t* out_diagnostic,
    occt_bridge_shape_id_t* out_shape) {
    return guarded(
        session,
        [&] {
            if (out_diagnostic == nullptr) {
                return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_diagnostic is null");
            }
            if (out_shape != nullptr) {
                *out_shape = OCCT_BRIDGE_INVALID_SHAPE_ID;
            }
            if (index >= session->last_diagnostics.size()) {
                return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "diagnostic index is out of range");
            }
            const occt_bridge_diagnostic_record& record = session->last_diagnostics[index];
            *out_diagnostic = {record.kind, record.code, record.input_index, record.shape.IsNull() ? 0 : 1};
            if (out_shape != nullptr && !record.shape.IsNull()) {
                /* Storing succeeds by clearing the last error; keep the failure's. */
                std::string error = session->last_error;
                const occt_bridge_status_t status = store_shape(session, record.shape, out_shape);
                if (status == OCCT_BRIDGE_OK) {
                    session->last_error = std::move(error);
                }
                return status;
            }
            return occt_bridge_status_t{OCCT_BRIDGE_OK};
        },
        true);
}

size_t occt_bridge_session_diagnostic_name(
    const occt_bridge_session_t* session,
    size_t index,
    char* buffer,
    size_t buffer_capacity) {
    if (session == nullptr) {
        return 0;
    }
    try {
        std::lock_guard<std::mutex> lock(session->mutex);
        if (index >= session->last_diagnostics.size()) {
            return 0;
        }
        return copy_text(session->last_diagnostics[index].name, buffer, buffer_capacity);
    } catch (...) {
        return 0;
    }
}

occt_bridge_status_t occt_bridge_session_get_options(
    occt_bridge_session_t* session,
    occt_bridge_session_options_t* out_options) {
    return guarded(session, [&] {
        if (out_options == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "out_options is null");
        }
        *out_options = session->options;
        return succeed(session);
    });
}

occt_bridge_status_t occt_bridge_session_set_options(
    occt_bridge_session_t* session,
    const occt_bridge_session_options_t* options) {
    return guarded(session, [&] {
        if (options == nullptr) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "options is null");
        }
        const auto is_flag = [](int value) { return value == 0 || value == 1; };
        if (!is_flag(options->validate_results) || !is_flag(options->heal_invalid_results)
            || !std::isfinite(options->boolean_fuzzy_tolerance) || options->boolean_fuzzy_tolerance < 0.0) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "invalid session options");
        }
        if (options->heal_invalid_results == 1 && options->validate_results == 0) {
            return fail(session, OCCT_BRIDGE_INVALID_ARGUMENT, "healing requires result validation");
        }
        session->options = *options;
        return succeed(session);
    });
}

occt_bridge_status_t occt_bridge_shape_remove(
    occt_bridge_session_t* session,
    occt_bridge_shape_id_t shape) {
    return guarded(session, [&] {
        if (session->shapes.erase(shape) == 0) {
            return fail(session, OCCT_BRIDGE_SHAPE_NOT_FOUND, "shape was not found");
        }
        session->histories.erase(shape);
        return succeed(session);
    });
}

void occt_bridge_shape_release(occt_bridge_session_t* session, occt_bridge_shape_id_t shape) {
    if (session == nullptr) {
        return;
    }
    try {
        std::lock_guard<std::mutex> lock(session->mutex);
        session->shapes.erase(shape);
        session->histories.erase(shape);
    } catch (...) {  // NOLINT(bugprone-empty-catch): release has no status to report and must not throw across the C ABI.
    }
}

}  // extern "C"
