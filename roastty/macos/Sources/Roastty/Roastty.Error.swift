extension Roastty {
    /// Possible errors from internal Roastty calls.
    enum Error: Swift.Error, CustomLocalizedStringResourceConvertible {
        case apiFailed

        var localizedStringResource: LocalizedStringResource {
            switch self {
            case .apiFailed: return "libroastty API call failed"
            }
        }
    }
}
