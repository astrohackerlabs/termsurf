import AppKit

extension UserDefaults {
    private static let customIconKeyOld = "CustomRoasttyIcon"
    private static let customIconKeyNew = "CustomRoasttyIcon2"

    var appIcon: AppIcon? {
        get {
            // Always remove our old pre-docktileplugin values.
            defer {
                removeObject(forKey: Self.customIconKeyOld)
            }

            // Check if we have the new key for our dock tile plugin format.
            guard let data = data(forKey: Self.customIconKeyNew) else {
                return nil
            }
            return try? JSONDecoder().decode(AppIcon.self, from: data)
        }

        set {
            guard let newData = try? JSONEncoder().encode(newValue) else {
                return
            }

            set(newData, forKey: Self.customIconKeyNew)
        }
    }
}
