import SwiftUI
import PhotosUI
import FondKit

/// Create or edit a recipe. Writes back to the canonical `.cook` file through
/// the Rust core (`FondClient`), so every edit round-trips losslessly through
/// Cooklang and the SQLite index is kept in sync. All parsing/writing logic
/// lives in Rust — this view is presentation only (ADR-011).
struct RecipeEditView: View {
    enum Mode: Equatable {
        case create
        case edit(slug: String)

        var isEditing: Bool { if case .edit = self { return true }; return false }
        var slug: String? { if case .edit(let s) = self { return s }; return nil }
    }

    let mode: Mode
    /// Called after a successful create/save with the (possibly renamed) slug.
    var onSaved: (String) -> Void = { _ in }
    /// Called after the recipe is deleted.
    var onDeleted: () -> Void = {}

    @EnvironmentObject private var model: AppModel
    @Environment(\.dismiss) private var dismiss

    // Editable metadata.
    @State private var title = ""
    @State private var servings = ""
    @State private var description = ""
    @State private var source = ""
    @State private var tags: [String] = []
    @State private var newTag = ""

    // Editable body (steps/sections/notes), preserving order and non-step blocks.
    @State private var blocks: [EditableBlock] = [EditableBlock(kind: .step, text: "")]

    // Optimistic-concurrency token + current photo link (edit mode).
    @State private var baseContentHash = ""
    @State private var image: String?

    // Derived, read-only ingredient preview.
    @State private var ingredients: [IngredientDto] = []

    // Photo picking.
    @State private var photoItem: PhotosPickerItem?

    // UI state.
    @State private var isLoading = false
    @State private var isSaving = false
    @State private var alert: EditAlert?
    @State private var showDeleteConfirm = false

    var body: some View {
        NavigationStack {
            Form {
                detailsSection
                tagsSection
                stepsSection
                ingredientsSection
                if mode.isEditing { photoSection; deleteSection }
            }
            .navigationTitle(mode.isEditing ? "Edit Recipe" : "New Recipe")
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") { save() }
                        .disabled(!canSave)
                }
            }
            .overlay { if isLoading { ProgressView() } }
            .task { if mode.isEditing { loadEditor() } ; refreshPreview() }
            .onChange(of: photoItem) { _, item in if let item { attachPhoto(item) } }
            .alert(
                alert?.title ?? "",
                isPresented: Binding(get: { alert != nil }, set: { if !$0 { alert = nil } }),
                presenting: alert
            ) { current in
                if current.kind == .conflict {
                    Button("Reload") { reloadFromDisk() }
                    Button("Cancel", role: .cancel) {}
                } else {
                    Button("OK", role: .cancel) {}
                }
            } message: { current in
                Text(current.message)
            }
            .confirmationDialog("Delete this recipe?", isPresented: $showDeleteConfirm,
                                titleVisibility: .visible) {
                Button("Delete Recipe", role: .destructive) { deleteRecipe() }
                Button("Cancel", role: .cancel) {}
            } message: {
                Text("The .cook file will be removed. This can’t be undone.")
            }
        }
    }

    private var canSave: Bool {
        !title.trimmingCharacters(in: .whitespaces).isEmpty && !isSaving
    }

    // MARK: - Sections

    private var detailsSection: some View {
        Section("Details") {
            TextField("Title", text: $title)
            TextField("Servings (e.g. 4)", text: $servings)
            TextField("Description", text: $description, axis: .vertical)
                .lineLimit(2...5)
            TextField("Source", text: $source)
        }
    }

    private var tagsSection: some View {
        Section("Tags") {
            ForEach(tags, id: \.self) { tag in
                Text(tag)
            }
            .onDelete { tags.remove(atOffsets: $0) }
            HStack {
                TextField("Add tag", text: $newTag)
                    .onSubmit(addTag)
                Button("Add", action: addTag)
                    .disabled(newTag.trimmingCharacters(in: .whitespaces).isEmpty)
            }
        }
    }

    private var stepsSection: some View {
        Section {
            ForEach($blocks) { $block in
                VStack(alignment: .leading, spacing: 4) {
                    if block.kind != .step {
                        Text(block.kindLabel.uppercased())
                            .font(.caption2).foregroundStyle(.secondary)
                    }
                    TextField(block.placeholder, text: $block.text, axis: .vertical)
                        .lineLimit(1...8)
                        .onChange(of: block.text) { _, _ in refreshPreview() }
                }
            }
            .onDelete { offsets in
                blocks.remove(atOffsets: offsets)
                refreshPreview()
            }
            .onMove { blocks.move(fromOffsets: $0, toOffset: $1) }
            Button {
                blocks.append(EditableBlock(kind: .step, text: ""))
            } label: {
                Label("Add step", systemImage: "plus")
            }
        } header: {
            Text("Steps")
        } footer: {
            Text("Write ingredients inline with Cooklang: @salt{1%tsp}, @onion{2}. "
                 + "Start a line with = for a section header.")
        }
    }

    private var ingredientsSection: some View {
        Section("Ingredients") {
            if ingredients.isEmpty {
                Text("Add @ingredient{quantity%unit} markup to your steps to build the list.")
                    .font(.caption).foregroundStyle(.secondary)
            } else {
                ForEach(Array(ingredients.enumerated()), id: \.offset) { _, ing in
                    HStack(alignment: .firstTextBaseline) {
                        Text(ing.name + (ing.optional ? " (optional)" : ""))
                        Spacer()
                        Text(compose(ing.quantity, ing.unit)).foregroundStyle(.secondary)
                    }
                }
            }
        }
    }

    private var photoSection: some View {
        Section {
            if let image {
                Label(image, systemImage: "photo").font(.caption).foregroundStyle(.secondary)
            }
            PhotosPicker(selection: $photoItem, matching: .images) {
                Label(image == nil ? "Attach photo" : "Replace photo", systemImage: "photo.badge.plus")
            }
        } header: {
            Text("Photo")
        } footer: {
            Text("Attaching a photo saves it immediately and reloads the recipe from disk.")
        }
    }

    private var deleteSection: some View {
        Section {
            Button(role: .destructive) { showDeleteConfirm = true } label: {
                Label("Delete Recipe", systemImage: "trash")
            }
        }
    }

    // MARK: - Tag helpers

    private func addTag() {
        let trimmed = newTag.trimmingCharacters(in: .whitespaces).lowercased()
        guard !trimmed.isEmpty, !tags.contains(trimmed) else { newTag = ""; return }
        tags.append(trimmed)
        newTag = ""
    }

    private func compose(_ quantity: String?, _ unit: String?) -> String {
        [quantity, unit].compactMap { $0 }.joined(separator: " ")
    }

    // MARK: - Data

    private func loadEditor() {
        guard let client = model.client, let slug = mode.slug else { return }
        isLoading = true
        defer { isLoading = false }
        do {
            guard let editor = try client.getRecipeForEdit(slug: slug) else { return }
            apply(editor)
        } catch {
            alert = EditAlert(kind: .error, message: String(describing: error))
        }
    }

    private func apply(_ editor: RecipeEditorDto) {
        title = editor.title
        servings = editor.servings ?? ""
        description = editor.description ?? ""
        source = editor.source ?? ""
        tags = editor.tags
        image = editor.image
        baseContentHash = editor.contentHash
        blocks = editor.blocks.map { EditableBlock(kind: $0.kind, text: $0.text, section: $0.section) }
        if blocks.isEmpty { blocks = [EditableBlock(kind: .step, text: "")] }
        ingredients = editor.ingredients
    }

    /// Recompute the ingredient preview from the current body via the Rust core.
    private func refreshPreview() {
        guard let client = model.client else { return }
        let texts = blocks.map(\.text).filter { !$0.trimmingCharacters(in: .whitespaces).isEmpty }
        ingredients = (try? client.previewIngredients(blocks: texts)) ?? ingredients
    }

    private func reloadFromDisk() {
        loadEditor()
        refreshPreview()
    }

    private func save() {
        guard let client = model.client else { return }
        let trimmedTitle = title.trimmingCharacters(in: .whitespaces)
        guard !trimmedTitle.isEmpty else { return }
        isSaving = true
        defer { isSaving = false }
        do {
            let saved: RecipeDto
            switch mode {
            case .create:
                let dto = NewRecipeDto(
                    title: trimmedTitle,
                    servings: optional(servings),
                    tags: tags,
                    description: optional(description),
                    source: optional(source),
                    steps: blocks.map(\.text).filter { !$0.trimmingCharacters(in: .whitespaces).isEmpty }
                )
                saved = try client.createRecipe(input: dto)
            case .edit(let slug):
                let dto = SaveRecipeDto(
                    slug: slug,
                    baseContentHash: baseContentHash,
                    title: trimmedTitle,
                    servings: optional(servings),
                    description: optional(description),
                    source: optional(source),
                    sourceUrl: nil,
                    prepTime: nil,
                    cookTime: nil,
                    totalTime: nil,
                    image: image,
                    tags: tags,
                    blocks: blocks.map { CookBlockDto(kind: $0.kind, text: $0.text, section: $0.section) }
                )
                saved = try client.saveRecipe(input: dto)
            }
            model.reload()
            onSaved(saved.slug)
            dismiss()
        } catch let error as FondError {
            handle(error)
        } catch {
            alert = EditAlert(kind: .error, message: String(describing: error))
        }
    }

    private func attachPhoto(_ item: PhotosPickerItem) {
        guard let client = model.client, let slug = mode.slug else { return }
        Task {
            do {
                guard let data = try await item.loadTransferable(type: Data.self) else { return }
                let ext = item.supportedContentTypes.first?.preferredFilenameExtension ?? "jpg"
                _ = try client.attachPhoto(
                    slug: slug,
                    bytes: data,
                    extension: ext,
                    baseContentHash: baseContentHash
                )
                // The write changed the file on disk; resync editor state.
                loadEditor()
                model.reload()
            } catch let error as FondError {
                handle(error)
            } catch {
                alert = EditAlert(kind: .error, message: String(describing: error))
            }
            photoItem = nil
        }
    }

    private func deleteRecipe() {
        guard let client = model.client, let slug = mode.slug else { return }
        do {
            _ = try client.deleteRecipe(slug: slug)
            model.reload()
            onDeleted()
            dismiss()
        } catch {
            alert = EditAlert(kind: .error, message: String(describing: error))
        }
    }

    private func handle(_ error: FondError) {
        if case .Conflict(let message) = error {
            alert = EditAlert(kind: .conflict, message: message)
        } else {
            alert = EditAlert(kind: .error, message: String(describing: error))
        }
    }

    private func optional(_ value: String) -> String? {
        let trimmed = value.trimmingCharacters(in: .whitespaces)
        return trimmed.isEmpty ? nil : trimmed
    }
}

// MARK: - Local edit models

/// A single editable body block. Identity is UI-local so `ForEach`/reorder is
/// stable while the user edits; the block's kind is re-derived from its text by
/// the Rust core on save.
private struct EditableBlock: Identifiable {
    let id = UUID()
    var kind: CookBlockKindDto
    var text: String
    var section: String?

    var kindLabel: String {
        switch kind {
        case .step: return "Step"
        case .section: return "Section"
        case .note: return "Note"
        case .comment: return "Comment"
        case .blockComment: return "Comment"
        }
    }

    var placeholder: String {
        switch kind {
        case .section: return "= Section name"
        case .note: return "> A tip or note"
        default: return "Describe this step…"
        }
    }
}

private struct EditAlert: Identifiable {
    enum Kind { case conflict, error }
    let id = UUID()
    let kind: Kind
    let message: String

    var title: String {
        switch kind {
        case .conflict: return "Recipe changed on disk"
        case .error: return "Couldn’t save"
        }
    }
}
