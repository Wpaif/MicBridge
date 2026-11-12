pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

// Textos da janela no idioma do sistema (LC_ALL, LC_MESSAGES ou LANG).
// Cada idioma é um JSON plano `chave -> texto` em i18n/, com variáveis em {nome}.
// Sem um idioma disponível, usa inglês, como a página do celular.
Singleton {
    id: root

    readonly property var available: ["pt-BR", "en"]
    readonly property string fallback: "en"
    readonly property string lang: pick(Quickshell.env("LC_ALL") || Quickshell.env("LC_MESSAGES") || Quickshell.env("LANG") || "")
    property var texts: ({})

    // "pt_BR.UTF-8" -> "pt-BR"; "pt_PT" e "pt" também caem em "pt-BR".
    function pick(locale) {
        var tag = String(locale).split(".")[0].split("@")[0].replace("_", "-")
        for (var i = 0; i < available.length; i++)
            if (available[i].toLowerCase() === tag.toLowerCase()) return available[i]
        var language = tag.split("-")[0].toLowerCase()
        for (var j = 0; j < available.length; j++)
            if (available[j].split("-")[0].toLowerCase() === language) return available[j]
        return fallback
    }

    // Texto da chave com as variáveis trocadas; a própria chave quando falta.
    function t(key, vars) {
        var text = texts[key] !== undefined ? texts[key] : key
        return text.replace(/\{(\w+)\}/g, function(m, name) {
            return vars && vars[name] !== undefined ? String(vars[name]) : m
        })
    }

    // Plural simples (pt e en): `<chave>.one` para 1, `<chave>.other` para o resto.
    function plural(key, n, vars) {
        var all = Object.assign({ n: n }, vars || {})
        return t(key + (n === 1 ? ".one" : ".other"), all)
    }

    // Número com o separador decimal do idioma.
    function num(value, digits) {
        return Number(value).toFixed(digits || 0).replace(".", t("format.decimal"))
    }

    FileView {
        path: String(Qt.resolvedUrl("i18n/" + root.lang + ".json")).replace(/^file:\/\//, "")
        blockLoading: true
        printErrors: true
        onLoaded: {
            try { root.texts = JSON.parse(text()) }
            catch (e) { console.warn("i18n/" + root.lang + ".json inválido: " + e) }
        }
    }
}
