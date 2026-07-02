import SwiftUI
import FondKit

/// Full recipe view: metadata, scalable ingredients, and steps. Entry point to
/// cook mode.
struct RecipeDetailView: View {
    let slug: String
    @Binding var selectedSlug: String?
    @EnvironmentObject private var model: AppModel

    @State private var recipe: RecipeDto?
    @State private var scaled: ScaledRecipeDto?
    @State private var multiplier: Double = 1.0
    @State private var error: String?
    @State private var showingEdit = false

    private let multipliers: [Double] = [0.5, 1.0, 2.0, 3.0]

    var body: some View {
        Group {
            if let recipe {
                content(recipe)
            } else if let error {
                ContentUnavailableView("Couldn’t load recipe", systemImage: "exclamationmark.triangle", description: Text(error))
            } else {
                ProgressView()
            }
        }
        .navigationTitle(recipe?.title ?? "Recipe")
        .toolbar {
            if recipe != nil {
                ToolbarItem {
                    Button { showingEdit = true } label: {
                        Label("Edit", systemImage: "pencil")
                    }
                }
            }
        }
        .sheet(isPresented: $showingEdit) {
            RecipeEditView(
                mode: .edit(slug: slug),
                onSaved: { newSlug in
                    if newSlug == slug {
                        load()
                    } else {
                        selectedSlug = newSlug
                    }
                },
                onDeleted: { selectedSlug = nil }
            )
            .environmentObject(model)
        }
        .task(id: slug) { load() }
    }

    @ViewBuilder
    private func content(_ recipe: RecipeDto) -> some View {
        List {
            if let desc = recipe.description, !desc.isEmpty {
                Section { Text(desc) }
            }

            Section("Details") {
                if let s = recipe.servings { labelRow("Servings", s) }
                if let t = recipe.totalTime { labelRow("Total time", t) }
                if let t = recipe.prepTime { labelRow("Prep", t) }
                if let t = recipe.cookTime { labelRow("Cook", t) }
                if let s = recipe.source, !s.isEmpty { labelRow("Source", s) }
            }

            Section {
                Picker("Scale", selection: $multiplier) {
                    ForEach(multipliers, id: \.self) { m in
                        Text(m == 1.0 ? "1× (original)" : "\(formatted(m))×").tag(m)
                    }
                }
                .pickerStyle(.segmented)
                .onChange(of: multiplier) { _, _ in rescale() }
            } header: {
                Text("Ingredients")
            }

            Section {
                ForEach(ingredientRows(recipe), id: \.id) { row in
                    HStack(alignment: .firstTextBaseline) {
                        Text(row.name)
                        Spacer()
                        Text(row.quantity).foregroundStyle(.secondary)
                    }
                }
            }

            Section("Steps") {
                ForEach(recipe.steps, id: \.order) { step in
                    VStack(alignment: .leading, spacing: 4) {
                        if let section = step.section, !section.isEmpty {
                            Text(section.uppercased())
                                .font(.caption).foregroundStyle(.secondary)
                        }
                        Text("\(step.order). \(step.body)")
                        if !step.timers.isEmpty {
                            ForEach(Array(step.timers.enumerated()), id: \.offset) { _, timer in
                                Label(timer.duration ?? (timer.name ?? "timer"),
                                      systemImage: "timer")
                                    .font(.caption).foregroundStyle(.orange)
                            }
                        }
                    }
                    .padding(.vertical, 2)
                }
            }

            Section {
                NavigationLink {
                    CookModeView(slug: slug, title: recipe.title)
                } label: {
                    Label("Start cook mode", systemImage: "flame")
                }
                .keyboardShortcut("r", modifiers: .command)
            }
        }
        #if os(iOS)
        .listStyle(.insetGrouped)
        #endif
    }

    // MARK: - Ingredient rows (scaled or original)

    private struct IngredientRow: Identifiable {
        let id = UUID()
        let name: String
        let quantity: String
    }

    private func ingredientRows(_ recipe: RecipeDto) -> [IngredientRow] {
        if let scaled {
            return scaled.ingredients.map { ing in
                IngredientRow(
                    name: ing.name + (ing.optional ? " (optional)" : ""),
                    quantity: compose(ing.scaledQuantity, ing.unit)
                )
            }
        }
        return recipe.ingredients.map { ing in
            IngredientRow(
                name: ing.name + (ing.optional ? " (optional)" : ""),
                quantity: compose(ing.quantity, ing.unit)
            )
        }
    }

    private func compose(_ quantity: String?, _ unit: String?) -> String {
        [quantity, unit].compactMap { $0 }.joined(separator: " ")
    }

    private func labelRow(_ label: String, _ value: String) -> some View {
        HStack {
            Text(label).foregroundStyle(.secondary)
            Spacer()
            Text(value)
        }
    }

    private func formatted(_ m: Double) -> String {
        m == m.rounded() ? String(Int(m)) : String(m)
    }

    // MARK: - Data

    private func load() {
        guard let client = model.client else { return }
        do {
            recipe = try client.getRecipe(slug: slug)
            rescale()
        } catch {
            self.error = String(describing: error)
        }
    }

    private func rescale() {
        guard let client = model.client else { return }
        if multiplier == 1.0 {
            scaled = nil
            return
        }
        scaled = try? client.scaleRecipe(slug: slug, factor: .multiplier(value: multiplier))
    }
}
