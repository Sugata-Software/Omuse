#pragma once

#include <QDialog>
#include <QJsonObject>
#include "interfaces/IPlatformServices.h"
#include <functional>
#include <vector>

// Dialogs describe edits. The caller owns dispatch, document state, and errors.
// They depend on callbacks, not the Swift ABI or SessionWindow.
class SizeDialog final : public QDialog {
public:
    // `colors` is the injected colour-choice service (defaults to the Qt picker).
    SizeDialog(const QJsonObject &state, bool imageSize, QWidget *parent = nullptr,
               std::shared_ptr<IColorPickerService> colors = nullptr);
    QJsonObject command() const;
private:
    std::function<QJsonObject()> m_command;
};

class FilterDialog final : public QDialog {
public:
    using Submit = std::function<bool(const QJsonObject &)>;
    FilterDialog(const QString &kind, Submit submit, QWidget *parent = nullptr);
};

// Everything an adjustment sheet needs from its host, injected rather than reached for (DIP).
struct AdjustServices {
    // Returns 256 bins for a channel (0 = RGB luma, 1 = R, 2 = G, 3 = B) of the pixels the adjustment starts from.
    using HistogramProvider = std::function<std::vector<double>(int channel)>;
    // Asks the host for the next pixel the user clicks on the canvas (original, un-adjusted colour).
    using SampleRequest = std::function<void(std::function<void(const QColor &)>)>;
    HistogramProvider histogram;
    std::shared_ptr<IColorPickerService> colors;
    SampleRequest requestSample;
};

class AdjustDialog final : public QDialog {
public:
    using Submit = std::function<bool(const QJsonObject &)>;
    using HistogramProvider = AdjustServices::HistogramProvider;
    AdjustDialog(const QString &kind, Submit submit, QWidget *parent = nullptr, AdjustServices services = {});
};

class SessionWindow;
class QLineEdit;
class QListWidget;
class QAction;

class CommandPaletteDialog final : public QDialog {
public:
    explicit CommandPaletteDialog(SessionWindow *window, QWidget *parent = nullptr);
protected:
    bool eventFilter(QObject *watched, QEvent *event) override;
private:
    struct CommandEntry {
        QString title;
        QString category;
        QString shortcut;
        QAction *action = nullptr;
    };
    SessionWindow *m_window = nullptr;
    QLineEdit *m_filter = nullptr;
    QListWidget *m_list = nullptr;
    std::vector<CommandEntry> m_entries;
    void collectCommands();
    void populateList(const QString &query);
    void triggerSelected();
};

