// XPC Gateway for TermSurf (Issue 506).
//
// Tiny daemon that owns the com.termsurf.xpc-gateway Mach service.
// The TermSurf app registers an anonymous listener endpoint here.
// `web` processes connect and claim the endpoint to talk directly to the app.

import Foundation

let serviceName = "com.termsurf.xpc-gateway"

// Global state — must be retained to prevent ARC release.
var peers: [xpc_connection_t] = []
var appEndpoint: xpc_object_t? = nil

let queue = DispatchQueue(label: "com.termsurf.xpc-gateway")

let listener = xpc_connection_create_mach_service(
    serviceName,
    queue,
    UInt64(XPC_CONNECTION_MACH_SERVICE_LISTENER))

xpc_connection_set_event_handler(listener) { object in
    guard xpc_get_type(object) == XPC_TYPE_CONNECTION else {
        fputs("[xpc-gateway] Listener error\n", stderr)
        return
    }

    let peer = object as xpc_connection_t
    peers.append(peer)
    fputs("[xpc-gateway] Peer connected (\(peers.count) total)\n", stderr)

    xpc_connection_set_event_handler(peer) { event in
        if xpc_get_type(event) == XPC_TYPE_DICTIONARY {
            handleMessage(event, from: peer)
        } else if xpc_get_type(event) == XPC_TYPE_ERROR {
            if event === XPC_ERROR_CONNECTION_INVALID {
                peers.removeAll { $0 === peer }
                fputs("[xpc-gateway] Peer disconnected (\(peers.count) remaining)\n", stderr)
            }
        }
    }
    xpc_connection_resume(peer)
}

xpc_connection_resume(listener)
fputs("[xpc-gateway] Listening on \(serviceName)\n", stderr)

func handleMessage(_ msg: xpc_object_t, from peer: xpc_connection_t) {
    guard let actionPtr = xpc_dictionary_get_string(msg, "action") else {
        fputs("[xpc-gateway] Message missing 'action'\n", stderr)
        return
    }
    let action = String(cString: actionPtr)

    switch action {
    case "register_app":
        guard let endpoint = xpc_dictionary_get_value(msg, "endpoint") else {
            fputs("[xpc-gateway] register_app missing endpoint\n", stderr)
            return
        }
        appEndpoint = endpoint
        fputs("[xpc-gateway] App registered endpoint\n", stderr)

    case "connect":
        let reply = xpc_dictionary_create_reply(msg)!
        if let endpoint = appEndpoint {
            xpc_dictionary_set_value(reply, "endpoint", endpoint)
            fputs("[xpc-gateway] Returning app endpoint to web process\n", stderr)
        } else {
            xpc_dictionary_set_string(reply, "error", "no_app")
            fputs("[xpc-gateway] No app registered yet\n", stderr)
        }
        xpc_connection_send_message(peer, reply)

    default:
        fputs("[xpc-gateway] Unknown action: \(action)\n", stderr)
    }
}

dispatchMain()
