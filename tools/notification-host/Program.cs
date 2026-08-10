using System.Globalization;
using System.Text.Json;
using System.Text.Json.Serialization;
using CommunityToolkit.WinUI.Notifications;
using Windows.UI.Notifications;

internal static class Program
{
    private const string ReminderGroup = "startodo-reminders";
    private const int MaxIdLength = 64;
    private const int MaxTitleLength = 256;
    private const int MaxBodyLength = 1_024;
    private const int MaxActivationUriLength = 512;
    private const int MaxDeliveryLength = 128;
    private const int MinTokenLength = 16;
    private const int MaxTokenLength = 256;

    private static readonly TimeSpan RequestReadTimeout = TimeSpan.FromSeconds(5);
    private static readonly object OutputLock = new();

    private static readonly JsonSerializerOptions JsonOptions = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
        DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull,
    };

    private static async Task<int> Main()
    {
        try
        {
            if (!Console.IsInputRedirected)
            {
                return 0;
            }

            var readTask = Task.Run(Console.In.ReadLine);
            if (await Task.WhenAny(readTask, Task.Delay(RequestReadTimeout)) != readTask)
            {
                Console.Error.WriteLine("Timed out waiting for notification-host request input.");
                WriteJson(new ErrorResponse(null, "input_timeout", "Timed out waiting for one JSON request line."));
                return 1;
            }

            var input = await readTask;
            if (string.IsNullOrWhiteSpace(input))
            {
                WriteJson(new ErrorResponse(null, "invalid_request", "Expected one non-empty JSON request line."));
                return 1;
            }

            using var document = JsonDocument.Parse(input);
            if (document.RootElement.ValueKind != JsonValueKind.Object)
            {
                WriteJson(new ErrorResponse(null, "invalid_request", "The request must be a JSON object."));
                return 1;
            }

            var request = ParseRequest(document.RootElement);
            var response = Dispatch(request);
            WriteJson(response);
            return response is ErrorResponse ? 1 : 0;
        }
        catch (JsonException exception)
        {
            Console.Error.WriteLine($"Invalid request JSON: {exception.Message}");
            WriteJson(new ErrorResponse(null, "invalid_json", "Request is not valid JSON."));
            return 1;
        }
        catch (Exception exception)
        {
            Console.Error.WriteLine(exception);
            WriteJson(new ErrorResponse(null, "internal_error", "Notification host failed to process the request."));
            return 1;
        }
    }

    private static HostRequest ParseRequest(JsonElement root)
    {
        var operation = ReadRequiredString(root, "operation");
        var id = ReadOptionalString(root, "id");
        var title = ReadOptionalString(root, "title");
        var body = ReadOptionalString(root, "body");
        var dueAtUtc = ReadOptionalString(root, "dueAtUtc");
        var activationUri = ReadOptionalString(root, "activationUri");
        return new HostRequest(operation, id, title, body, dueAtUtc, activationUri);
    }

    private static object Dispatch(HostRequest request)
    {
        return request.Operation switch
        {
            "diagnostics" => Diagnostics(),
            "show" => Show(request),
            "schedule" => Schedule(request),
            "cancel" => Cancel(request),
            "list" => List(),
            _ => new ErrorResponse(request.Operation, "unsupported_operation", "Supported operations are diagnostics, show, schedule, cancel, and list."),
        };
    }

    private static object Diagnostics()
    {
        var notifier = ToastNotificationManagerCompat.CreateToastNotifier();
        var pendingCount = notifier.GetScheduledToastNotifications()
            .Count(notification => string.Equals(notification.Group, ReminderGroup, StringComparison.Ordinal));

        return new DiagnosticsResponse(
            "diagnostics",
            notifier.Setting.ToString(),
            pendingCount,
            false);
    }

    private static object Show(HostRequest request)
    {
        var validationError = ValidateReminder(request, requireDueAtUtc: false, out _, out var activationUri);
        if (validationError is not null)
        {
            return validationError;
        }

        var toast = new ToastContentBuilder()
            .SetProtocolActivation(activationUri!)
            .AddText(request.Title!)
            .AddText(request.Body!)
            .GetToastContent();

        ToastNotificationManagerCompat.CreateToastNotifier().Show(new ToastNotification(toast.GetXml()));
        return new SuccessResponse("show", request.Id);
    }

    private static object Schedule(HostRequest request)
    {
        var validationError = ValidateReminder(request, requireDueAtUtc: true, out var dueAtUtc, out var activationUri);
        if (validationError is not null)
        {
            return validationError;
        }

        var notifier = ToastNotificationManagerCompat.CreateToastNotifier();
        foreach (var existing in notifier.GetScheduledToastNotifications()
                     .Where(notification => string.Equals(notification.Tag, request.Id, StringComparison.Ordinal)
                         && string.Equals(notification.Group, ReminderGroup, StringComparison.Ordinal)))
        {
            notifier.RemoveFromSchedule(existing);
        }

        var toast = new ToastContentBuilder()
            .SetProtocolActivation(activationUri!)
            .AddText(request.Title!)
            .AddText(request.Body!)
            .GetToastContent();

        var scheduled = new ScheduledToastNotification(toast.GetXml(), dueAtUtc!.Value.UtcDateTime)
        {
            Tag = request.Id!,
            Group = ReminderGroup,
        };

        notifier.AddToSchedule(scheduled);
        return new ScheduleResponse("schedule", request.Id!, dueAtUtc.Value.ToString("O", CultureInfo.InvariantCulture));
    }

    private static object Cancel(HostRequest request)
    {
        var idError = ValidateId(request.Operation, request.Id);
        if (idError is not null)
        {
            return idError;
        }

        var notifier = ToastNotificationManagerCompat.CreateToastNotifier();
        var removed = 0;
        foreach (var scheduled in notifier.GetScheduledToastNotifications()
                     .Where(notification => string.Equals(notification.Tag, request.Id, StringComparison.Ordinal)
                         && string.Equals(notification.Group, ReminderGroup, StringComparison.Ordinal)))
        {
            notifier.RemoveFromSchedule(scheduled);
            removed++;
        }

        return new CancelResponse("cancel", request.Id!, removed);
    }

    private static object List()
    {
        var scheduled = ToastNotificationManagerCompat.CreateToastNotifier()
            .GetScheduledToastNotifications()
            .Where(notification => string.Equals(notification.Group, ReminderGroup, StringComparison.Ordinal))
            .Select(notification => new ScheduledItem(
                notification.Tag,
                notification.Group,
                notification.DeliveryTime.ToUniversalTime().ToString("O", CultureInfo.InvariantCulture),
                ReadScheduledActivationUri(notification)))
            .OrderBy(notification => notification.DueAtUtc, StringComparer.Ordinal)
            .ToArray();

        return new ListResponse("list", scheduled);
    }

    private static string? ReadScheduledActivationUri(ScheduledToastNotification notification)
    {
        try
        {
            var activationUri = notification.Content.DocumentElement?.GetAttribute("launch");
            return ValidateActivationUri("list", activationUri, out _) is null ? activationUri : null;
        }
        catch (Exception exception)
        {
            Console.Error.WriteLine($"Could not read scheduled toast activation URI: {exception.Message}");
            return null;
        }
    }

    private static ErrorResponse? ValidateReminder(
        HostRequest request,
        bool requireDueAtUtc,
        out DateTimeOffset? dueAtUtc,
        out Uri? activationUri)
    {
        dueAtUtc = null;
        activationUri = null;
        var idError = ValidateId(request.Operation, request.Id);
        if (idError is not null)
        {
            return idError;
        }

        if (string.IsNullOrWhiteSpace(request.Title) || request.Title.Length > MaxTitleLength)
        {
            return new ErrorResponse(request.Operation, "invalid_title", $"title must contain 1 to {MaxTitleLength} characters.");
        }

        if (string.IsNullOrWhiteSpace(request.Body) || request.Body.Length > MaxBodyLength)
        {
            return new ErrorResponse(request.Operation, "invalid_body", $"body must contain 1 to {MaxBodyLength} characters.");
        }

        var activationUriError = ValidateActivationUri(request.Operation, request.ActivationUri, out activationUri);
        if (activationUriError is not null)
        {
            return activationUriError;
        }

        if (!requireDueAtUtc)
        {
            return null;
        }

        if (string.IsNullOrWhiteSpace(request.DueAtUtc)
            || !DateTimeOffset.TryParse(request.DueAtUtc, CultureInfo.InvariantCulture, DateTimeStyles.RoundtripKind, out var parsedDueAtUtc)
            || parsedDueAtUtc.Offset != TimeSpan.Zero)
        {
            return new ErrorResponse(request.Operation, "invalid_due_at_utc", "dueAtUtc must be an ISO 8601 UTC timestamp with a zero offset.");
        }

        var now = DateTimeOffset.UtcNow;
        if (parsedDueAtUtc <= now || parsedDueAtUtc > now.AddYears(1))
        {
            return new ErrorResponse(request.Operation, "invalid_due_at_utc", "dueAtUtc must be in the future and no more than one year from now.");
        }

        dueAtUtc = parsedDueAtUtc;
        return null;
    }

    private static ErrorResponse? ValidateId(string operation, string? id)
    {
        if (string.IsNullOrWhiteSpace(id)
            || id.Length > MaxIdLength
            || !id.All(character => char.IsAsciiLetterOrDigit(character) || character is '_' or '-'))
        {
            return new ErrorResponse(
                operation,
                "invalid_id",
                $"id must contain 1 to {MaxIdLength} ASCII letters, digits, '_' or '-' characters.");
        }

        return null;
    }

    private static ErrorResponse? ValidateActivationUri(string operation, string? activationUri, out Uri? validActivationUri)
    {
        validActivationUri = null;
        if (string.IsNullOrWhiteSpace(activationUri)
            || activationUri.Length > MaxActivationUriLength
            || !Uri.TryCreate(activationUri, UriKind.Absolute, out var parsedUri)
            || !string.Equals(parsedUri.Scheme, "startodo", StringComparison.OrdinalIgnoreCase)
            || !string.Equals(parsedUri.Host, "reminder", StringComparison.OrdinalIgnoreCase)
            || !string.Equals(parsedUri.AbsolutePath, "/open", StringComparison.Ordinal)
            || !string.IsNullOrEmpty(parsedUri.UserInfo)
            || !parsedUri.IsDefaultPort
            || !string.IsNullOrEmpty(parsedUri.Fragment))
        {
            return new ErrorResponse(operation, "invalid_activation_uri", "activationUri must be a valid startodo reminder activation URI.");
        }

        var query = parsedUri.Query;
        if (query.Length <= 1)
        {
            return new ErrorResponse(operation, "invalid_activation_uri", "activationUri must include exactly one delivery and token query parameter.");
        }

        string? delivery = null;
        string? token = null;
        foreach (var part in query[1..].Split('&', StringSplitOptions.None))
        {
            var separatorIndex = part.IndexOf('=');
            if (separatorIndex <= 0 || separatorIndex != part.LastIndexOf('='))
            {
                return new ErrorResponse(operation, "invalid_activation_uri", "activationUri must include exactly one delivery and token query parameter.");
            }

            var key = part[..separatorIndex];
            var value = part[(separatorIndex + 1)..];
            if (key == "delivery" && delivery is null)
            {
                delivery = value;
            }
            else if (key == "token" && token is null)
            {
                token = value;
            }
            else
            {
                return new ErrorResponse(operation, "invalid_activation_uri", "activationUri must include exactly one delivery and token query parameter.");
            }
        }

        if (!IsValidDelivery(delivery) || !IsValidBase64UrlToken(token))
        {
            return new ErrorResponse(operation, "invalid_activation_uri", "activationUri delivery or token is invalid.");
        }

        validActivationUri = parsedUri;
        return null;
    }

    private static bool IsValidDelivery(string? delivery)
    {
        return !string.IsNullOrEmpty(delivery)
            && delivery.Length <= MaxDeliveryLength
            && delivery.All(character => char.IsAsciiLetterOrDigit(character) || character is '_' or '-');
    }

    private static bool IsValidBase64UrlToken(string? token)
    {
        return !string.IsNullOrEmpty(token)
            && token.Length is >= MinTokenLength and <= MaxTokenLength
            && token.Length % 4 != 1
            && token.All(character => char.IsAsciiLetterOrDigit(character) || character is '_' or '-');
    }

    private static string ReadRequiredString(JsonElement root, string propertyName)
    {
        var value = ReadOptionalString(root, propertyName);
        if (string.IsNullOrWhiteSpace(value))
        {
            throw new JsonException($"Missing required string property '{propertyName}'.");
        }

        return value;
    }

    private static string? ReadOptionalString(JsonElement root, string propertyName)
    {
        if (!root.TryGetProperty(propertyName, out var property) || property.ValueKind == JsonValueKind.Null)
        {
            return null;
        }

        if (property.ValueKind != JsonValueKind.String)
        {
            throw new JsonException($"Property '{propertyName}' must be a string.");
        }

        return property.GetString();
    }

    private static void WriteJson<T>(T value)
    {
        var line = JsonSerializer.Serialize(value, JsonOptions);
        lock (OutputLock)
        {
            Console.Out.WriteLine(line);
            Console.Out.Flush();
        }
    }

    private sealed record HostRequest(string Operation, string? Id, string? Title, string? Body, string? DueAtUtc, string? ActivationUri);
    private sealed record ErrorResponse(string? Operation, string Code, string Message) { public bool Ok { get; } = false; }
    private sealed record SuccessResponse(string Operation, string? Id) { public bool Ok { get; } = true; }
    private sealed record ScheduleResponse(string Operation, string Id, string DueAtUtc) { public bool Ok { get; } = true; }
    private sealed record CancelResponse(string Operation, string Id, int Removed) { public bool Ok { get; } = true; }
    private sealed record DiagnosticsResponse(string Operation, string Setting, int PendingCount, bool ToastActivated) { public bool Ok { get; } = true; }
    private sealed record ListResponse(string Operation, IReadOnlyList<ScheduledItem> Items) { public bool Ok { get; } = true; }
    private sealed record ScheduledItem(string Tag, string Group, string DueAtUtc, string? ActivationUri);
}
